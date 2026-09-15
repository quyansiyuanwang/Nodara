/**
 * Studio entry point.
 *
 * Wiring only: connect, discover, render, execute. Everything with real logic
 * lives in `runtime/`, `model/` or `ui/`.
 */

import "./styles.css";

import {
  applyStaticTranslations,
  localizeDescriptor,
  localizeDiagnostic,
  localizeProblem,
  t,
  toggleLocale,
} from "./i18n";
import { WorkflowHistory } from "./model/history";
import {
  applyWorkflow,
  localProblems,
  starterWorkflow,
  nodeTypeAdmission,
  workflowAdmission,
  WORKFLOW_SCHEMA_PATH,
} from "./model/workflow";
import { defaultRuntimeBaseUrl, RuntimeClient, RuntimeError } from "./runtime/client";
import {
  Diagnostic,
  NodeDescriptor,
  RunStatus,
  Workflow,
} from "./runtime/types";
import { AgentPanel } from "./ui/agent-panel";
import { AuditPanel } from "./ui/audit-panel";
import { Canvas } from "./ui/canvas";
import { EventLog } from "./ui/event-log";
import { FeatureRegistry } from "./ui/feature-registry";
import { ExtensionPanel } from "./ui/extension-panel";
import { Inspector } from "./ui/inspector";
import { Palette } from "./ui/palette";
import { RegionPicker } from "./ui/region-picker";
import { installResizer } from "./ui/resizer";
import { RunDialog } from "./ui/run-dialog";
import { RunPanel } from "./ui/run-panel";
import { deriveRunControls, ValidationState } from "./ui/run-controls";

function element<T extends Element = HTMLElement>(id: string): T {
  const found = document.getElementById(id);
  if (!found) throw new Error(`missing element #${id}`);
  return found as unknown as T;
}

applyStaticTranslations();

class Studio {
  private readonly client = new RuntimeClient(defaultRuntimeBaseUrl());

  /**
   * The editor mutates this object in place rather than replacing it, because
   * the canvas, inspector and event log all hold a reference to it.
   */
  private readonly workflow: Workflow = starterWorkflow();

  private descriptors = new Map<string, NodeDescriptor>();
  private diagnostics: Diagnostic[] = [];
  private runId: string | null = null;
  private closeStream: (() => void) | null = null;
  private validateToken = 0;
  private validationTimer: number | null = null;
  private validationState: ValidationState = "unknown";
  private workflowRevision = 0;
  private auditToken = 0;
  private runsToken = 0;
  private extensionsToken = 0;
  private lastAgentPollError: string | null = null;
  private connected = false;
  private currentRunStatus: RunStatus | null = null;
  private runStarting = false;
  private readonly runOverrides = new Map<string, unknown>();

  private readonly palette: Palette;
  private readonly canvas: Canvas;
  private readonly inspector: Inspector;
  private readonly log: EventLog;
  private readonly agents: AgentPanel;
  private readonly audit: AuditPanel;
  private readonly runsPanel: RunPanel;
  private readonly extensionsPanel: ExtensionPanel;
  private readonly runDialog: RunDialog;
  private readonly regionPicker: RegionPicker;
  private readonly features = new FeatureRegistry();
  private agentPoll: number | null = null;
  private stepStarting = false;
  private history!: WorkflowHistory;
  private historyTimer: number | null = null;

  constructor() {
    const canvasElement = element<SVGSVGElement>("canvas");

    this.canvas = new Canvas(canvasElement, this.workflow, {
      onChange: () => this.workflowChanged(),
      onSelect: () => this.renderInspector(),
      onStatus: (message) => this.pushLocal(message),
      descriptorFor: (nodeType) => this.descriptors.get(nodeType),
      onViewChange: (scale) => this.updateZoomLabel(scale),
    });
    // The canvas must exist before the palette can call back into it, so the
    // palette is constructed with a lazy reference rather than a captured value.
    this.palette = new Palette(element("palette"), {
      onAdd: (descriptor) => this.canvas.addNodeAtViewportCenter(descriptor),
      onDragStart: (descriptor, event) => this.canvas.beginPaletteDrag(descriptor, event),
      allowed: (descriptor) => nodeTypeAdmission(this.workflow, descriptor.node_type),
    });
    this.regionPicker = new RegionPicker(
      element<HTMLDialogElement>("region-picker"),
      this.client,
    );
    this.inspector = new Inspector(
      element("inspector"),
      this.workflow,
      (nodeType) => this.descriptors.get(nodeType),
      {
        onChange: () => this.workflowChanged(),
        getRunOverride: (name) => this.runOverrides.get(name),
        setRunOverride: (name, value) => this.runOverrides.set(name, value),
        clearRunOverride: (name) => this.runOverrides.delete(name),
        pickCaptureRegion: () => this.regionPicker.pick(),
      },
    );
    this.log = new EventLog(
      element("events"),
      this.canvas,
      (runId, artifactId) => this.client.artifactUrl(runId, artifactId),
    );
    this.agents = new AgentPanel(element("agent"), {
      onDecide: (sessionId, approvalId, approve) =>
        void this.decideApproval(sessionId, approvalId, approve),
      onLoadPlan: (sessionId) => this.loadPlan(sessionId),
      onOpenRun: (runId) => void this.openRun(runId),
    });
    this.audit = new AuditPanel(element("audit"));
    this.runsPanel = new RunPanel(element("runs"), {
      onOpenRun: (runId) => void this.openRun(runId),
    });
    this.extensionsPanel = new ExtensionPanel(element("extensions"));
    this.runDialog = new RunDialog(
      element<HTMLDialogElement>("run-dialog"),
      this.workflow,
      {
        onRun: () => void this.run(),
        getOverride: (name) => this.runOverrides.get(name),
        setOverride: (name, value) => this.runOverrides.set(name, value),
        clearOverride: (name) => this.runOverrides.delete(name),
      },
    );
    this.history = new WorkflowHistory(JSON.stringify(this.workflow));

    this.registerBuiltinFeatures();
    this.renderFeatureTabs();
    this.bindToolbar();
    this.bindResizers();
    this.updateHistoryControls();
    this.renderInspector();
    this.renderWorkspace();
    this.setStatus(null);
  }

  async start(): Promise<void> {
    await this.connect();
    // The runtime may be started after the Studio; poll so the UI heals itself.
    window.setInterval(() => void this.connect(true), 5000);
  }

  private async connect(quiet = false): Promise<void> {
    const badge = element("connection");
    try {
      const health = await this.client.health();
      const plugins = await this.client.plugins();
      const extensions = await this.client.extensions();
      const descriptors = await this.client.nodeTypes();
      const firstConnection = !this.connected;
      this.connected = true;
      const localized = descriptors.map(localizeDescriptor);
      this.descriptors = new Map(localized.map((descriptor) => [descriptor.node_type, descriptor]));
      this.palette.setDescriptors(localized);
      this.extensionsPanel.setExtensions(extensions);

      const failed = plugins.failures.length;
      badge.textContent = t(failed ? "toolbar.nodeSummaryFailed" : "toolbar.nodeSummary", {
        nodes: health.node_types,
        plugins: plugins.plugins.length,
        failed,
      });
      badge.className = `connection connection--${failed ? "warn" : "ok"}`;
      badge.title = plugins.failures
        .map((failure) => `${failure.id}: ${failure.message}`)
        .join("\n");
      this.renderWorkspace();
      if (firstConnection) {
        this.scheduleValidation(0);
      } else if (!quiet) {
        this.scheduleValidation(350);
      }
    } catch (error) {
      this.connected = false;
      badge.textContent = t("toolbar.runtimeUnreachable");
      badge.className = "connection connection--error";
      badge.title = (error as Error).message;
      this.setValidationState(
        "unavailable",
        t("validation.runtimeUnavailable"),
      );
    }
  }

  private registerBuiltinFeatures(): void {
    const panels = [
      ["events", "tabs.events", "panel-events", 10],
      ["runs", "tabs.runs", "panel-runs", 20],
      ["extensions", "tabs.extensions", "panel-extensions", 30],
      ["agent", "tabs.agent", "panel-agent", 40],
      ["audit", "tabs.audit", "panel-audit", 50],
      ["problems", "tabs.problems", "panel-problems", 60],
      ["json", "tabs.json", "panel-json", 70],
    ] as const;
    for (const [id, labelKey, panelId, order] of panels) {
      this.features.registerPanel({ id, labelKey, panelId, order });
    }
  }

  private renderFeatureTabs(): void {
    const nav = element("drawer-tabs");
    nav.replaceChildren();
    for (const [index, feature] of this.features.panels().entries()) {
      const tab = document.createElement("button");
      tab.type = "button";
      tab.className = `tab${index === 0 ? " tab--active" : ""}`;
      tab.dataset.tab = feature.id;
      tab.textContent = t(feature.labelKey);
      nav.appendChild(tab);
    }
  }

  private bindToolbar(): void {
    element("btn-language").addEventListener("click", () => {
      toggleLocale();
      window.location.reload();
    });

    element("btn-new").addEventListener("click", () => {
      if (!confirm(t("dialog.discardWorkflow"))) return;
      this.replaceWorkflow(starterWorkflow());
    });

    element("btn-undo").addEventListener("click", () => {
      this.flushHistory();
      this.undo();
    });
    element("btn-redo").addEventListener("click", () => this.redo());

    element("btn-export").addEventListener("click", () => {
      const blob = new Blob([JSON.stringify(this.workflow, null, 2)], {
        type: "application/json",
      });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = `${this.workflow.id}.json`;
      link.click();
      URL.revokeObjectURL(url);
    });

    const fileInput = element<HTMLInputElement>("file-input");
    element("btn-import").addEventListener("click", () => fileInput.click());
    fileInput.addEventListener("change", async () => {
      const file = fileInput.files?.[0];
      if (!file) return;
      try {
        this.replaceWorkflow(JSON.parse(await file.text()) as Workflow);
      } catch (error) {
        this.pushLocal(t("status.couldNotImport", { message: (error as Error).message }));
      } finally {
        fileInput.value = "";
      }
    });

    element("btn-validate").addEventListener("click", () => void this.validate(true));
    element("validation-status").addEventListener("click", () => this.showTab("problems"));
    element("btn-run").addEventListener("click", () => void this.run());
    element("btn-run-options").addEventListener("click", () => this.runDialog.open());
    element("btn-pause").addEventListener("click", () => void this.control("pause"));
    element("btn-resume").addEventListener("click", () => void this.control("resume"));
    element("btn-step").addEventListener("click", () => void this.stepRun());
    element("btn-cancel").addEventListener("click", () => void this.control("cancel"));
    element("canvas-zoom-out").addEventListener("click", () => this.canvas.zoomOut());
    element("canvas-zoom-in").addEventListener("click", () => this.canvas.zoomIn());
    element("canvas-zoom-level").addEventListener("click", () => this.canvas.resetView());
    element("canvas-fit").addEventListener("click", () => this.canvas.fitToContent());
    element("canvas-layout").addEventListener("click", () => this.canvas.autoLayout());

    element<HTMLInputElement>("palette-filter").addEventListener("input", (event) => {
      this.palette.filter((event.target as HTMLInputElement).value);
    });
    window.addEventListener("keydown", (event) => this.onHistoryKeyDown(event));

    for (const tab of document.querySelectorAll<HTMLButtonElement>(".tab")) {
      tab.addEventListener("click", () => {
        const name = tab.dataset.tab ?? "events";
        this.showTab(name);
        if (name === "agent") void this.pollAgentSessions();
        if (name === "audit") void this.refreshAudit();
      });
    }

    element("audit-refresh").addEventListener("click", () => void this.refreshAudit());
    element("audit-current-run").addEventListener("change", () => void this.refreshAudit());
    element("runs-refresh").addEventListener("click", () => void this.refreshRuns());
    element("extensions-refresh").addEventListener("click", () => void this.refreshExtensions());

    element("btn-apply-json").addEventListener("click", () => {
      try {
        const replaced = this.replaceWorkflow(
          JSON.parse(element<HTMLTextAreaElement>("json-view").value) as Partial<Workflow>,
        );
        if (!replaced) return;
        this.pushLocal(t("status.workflowReplaced"));
      } catch (error) {
        this.pushLocal(t("status.invalidWorkflowJson", { message: (error as Error).message }));
      }
    });

    element("btn-runtime-schema").addEventListener("click", () => {
      this.workflow.$schema = this.runtimeSchemaUrl();
      this.workflowChanged();
      this.showTab("json");
      this.pushLocal(t("status.schemaUpdated", { schema: this.workflow.$schema ?? "" }));
    });
  }

  /**
   * The schema URL this deployment serves, which completes exactly the node
   * types the connected runtime installed rather than a static file.
   */
  private bindResizers(): void {
    const app = element("app");
    const left = element("resize-left");
    const right = element("resize-right");
    const drawer = element("resize-drawer");
    let leftWidth = 240;
    let rightWidth = 300;
    let storedDrawerHeight: number | null = null;
    try {
      const stored = localStorage.getItem("nodara.drawer.height");
      if (stored !== null) storedDrawerHeight = Number(stored);
    } catch {
      // Hardened webviews may disable local storage; keep the default.
    }
    const defaultDrawerHeight = Math.round(
      Math.max(280, Math.min(420, window.innerHeight * 0.34)),
    );
    let drawerHeight = storedDrawerHeight !== null && Number.isFinite(storedDrawerHeight)
      ? Math.max(180, Math.min(640, storedDrawerHeight))
      : defaultDrawerHeight;
    app.style.setProperty("--drawer-height", `${drawerHeight}px`);

    installResizer(left, {
      axis: "x",
      value: leftWidth,
      min: 170,
      max: () => Math.max(260, Math.min(460, window.innerWidth - rightWidth - 360)),
      onChange: (value) => {
        leftWidth = value;
        app.style.setProperty("--left-panel", `${value}px`);
      },
    });
    installResizer(right, {
      axis: "x",
      value: rightWidth,
      min: 220,
      max: () => Math.max(220, Math.min(560, window.innerWidth - leftWidth - 360)),
      invert: true,
      onChange: (value) => {
        rightWidth = value;
        app.style.setProperty("--right-panel", `${value}px`);
      },
    });
    installResizer(drawer, {
      axis: "y",
      value: drawerHeight,
      min: 140,
      max: () => Math.max(180, Math.min(640, window.innerHeight - 260)),
      invert: true,
      onChange: (value) => {
        drawerHeight = value;
        app.style.setProperty("--drawer-height", `${value}px`);
        try {
          localStorage.setItem("nodara.drawer.height", String(value));
        } catch {
          // Resizing still works for this session.
        }
      },
    });
  }
  private runtimeSchemaUrl(): string {
    return `${window.location.origin}/api/v1/schema/workflow`;
  }

  private replaceWorkflow(next: Partial<Workflow> | null): boolean {
    const admission = workflowAdmission({ nodes: next?.nodes ?? [] });
    if (!admission.allowed) {
      this.pushLocal(localizeProblem(admission.reason ?? ""));
      return false;
    }
    applyWorkflow(this.workflow, next);
    this.runOverrides.clear();
    this.canvas.select(null);
    this.workflowChanged();
    return true;
  }

  private showTab(name: string): void {
    const feature = this.features.getPanel(name);
    for (const tab of document.querySelectorAll<HTMLButtonElement>(".tab")) {
      tab.classList.toggle("tab--active", tab.dataset.tab === name);
    }
    for (const panel of document.querySelectorAll<HTMLElement>(".drawer__panel")) {
      panel.hidden = panel.id !== feature?.panelId;
    }
    if (name === "json") this.renderJson();
    if (name === "runs") void this.refreshRuns();
    if (name === "extensions") void this.refreshExtensions();
  }

  private updateZoomLabel(scale: number): void {
    element("canvas-zoom-level").textContent = `${Math.round(scale * 100)}%`;
  }

  private scheduleHistoryCommit(): void {
    if (this.historyTimer !== null) window.clearTimeout(this.historyTimer);
    this.historyTimer = window.setTimeout(() => {
      this.historyTimer = null;
      this.commitHistory();
    }, 250);
  }

  private flushHistory(): void {
    if (this.historyTimer !== null) {
      window.clearTimeout(this.historyTimer);
      this.historyTimer = null;
    }
    this.commitHistory();
  }

  private commitHistory(): void {
    this.history.push(JSON.stringify(this.workflow));
    this.updateHistoryControls();
  }

  private updateHistoryControls(): void {
    element<HTMLButtonElement>("btn-undo").disabled = !this.history.canUndo();
    element<HTMLButtonElement>("btn-redo").disabled = !this.history.canRedo();
  }

  private undo(): void {
    this.flushHistory();
    const snapshot = this.history.undo();
    if (snapshot !== null) this.restoreHistory(snapshot);
    this.updateHistoryControls();
  }

  private redo(): void {
    this.flushHistory();
    const snapshot = this.history.redo();
    if (snapshot !== null) this.restoreHistory(snapshot);
    this.updateHistoryControls();
  }

  private restoreHistory(snapshot: string): void {
    applyWorkflow(this.workflow, JSON.parse(snapshot) as Workflow);
    this.runOverrides.clear();
    this.diagnostics = [];
    this.workflowRevision += 1;
    this.canvas.select(null);
    this.palette.refreshAvailability();
    this.setValidationState("checking", t("validation.waiting"));
    this.renderWorkspace();
    this.scheduleValidation();
  }

  private onHistoryKeyDown(event: KeyboardEvent): void {
    const target = event.target as HTMLElement | null;
    if (target && ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName)) return;
    if (!(event.ctrlKey || event.metaKey)) return;
    const key = event.key.toLowerCase();
    if (key === "z") {
      event.preventDefault();
      this.flushHistory();
      if (event.shiftKey) this.redo();
      else this.undo();
    } else if (key === "y") {
      event.preventDefault();
      this.flushHistory();
      this.redo();
    }
  }

  private renderInspector(): void {
    this.inspector.render(
      this.canvas.selectedNodeId(),
      this.diagnostics,
      this.canvas.selectedEdgeId(),
    );
  }

  /** Render without treating a health poll as a document mutation. */
  private renderWorkspace(): void {
    this.canvas.render();
    this.renderJson();
    this.renderProblems();
    this.updateRunControls();
  }

  /** Called after every real workflow edit. */
  private workflowChanged(): void {
    this.diagnostics = [];
    this.workflowRevision += 1;
    this.palette.refreshAvailability();
    // Preserve the caret while a property field is being edited.
    if (!element("inspector").contains(document.activeElement)) {
      this.renderInspector();
    }
    this.setValidationState("checking", t("validation.waiting"));
    this.renderWorkspace();
    this.scheduleHistoryCommit();
    this.scheduleValidation();
  }

  private renderJson(): void {
    const view = element<HTMLTextAreaElement>("json-view");
    // The periodic health poll also re-renders; while the user is editing the
    // JSON that overwrite would wipe their in-progress edits, so it waits
    // until the textarea loses focus.
    if (document.activeElement !== view) {
      view.value = JSON.stringify(this.workflow, null, 2);
    }
    const hint = element("json-schema");
    const declared = this.workflow.$schema || WORKFLOW_SCHEMA_PATH;
    const origin = declared === WORKFLOW_SCHEMA_PATH ? t("schema.published") : declared;
    hint.textContent = t("schema.hint", {
      origin,
      count: this.descriptors.size || "no",
    });
  }

  private renderProblems(): void {
    const root = element("problems");
    root.replaceChildren();
    const problems: Array<{
      severity: "info" | "warning" | "error";
      text: string;
      nodeId?: string;
      edgeId?: string;
    }> = [
      ...localProblems(this.workflow).map((text) => ({
        severity: "error" as const,
        text: localizeProblem(text),
      })),
      ...this.diagnostics.map(localizeDiagnostic).map((diagnostic) => ({
        severity: diagnostic.severity,
        text: `[${diagnostic.code}] ${diagnostic.message} (${diagnostic.path})${
          diagnostic.hint ? ` — ${diagnostic.hint}` : ""
        }`,
        nodeId: diagnostic.node_id,
        edgeId: diagnostic.edge_id,
      })),
    ];
    const problemsTab = document.querySelector<HTMLButtonElement>('.tab[data-tab="problems"]');
    problemsTab?.classList.toggle(
      "tab--attention",
      problems.some((problem) => problem.severity === "error"),
    );
    if (problems.length === 0) {
      const ok = document.createElement("p");
      ok.className = "muted";
      ok.textContent = t("problems.none");
      root.appendChild(ok);
      return;
    }
    for (const problem of problems) {
      const target = problem.nodeId || problem.edgeId;
      const row = document.createElement(target ? "button" : "p");
      row.className = `problem problem--${problem.severity}`;
      if (row instanceof HTMLButtonElement) {
        row.type = "button";
        row.classList.add("problem--clickable");
        row.title = t("problems.locate");
        row.addEventListener("click", () => {
          this.canvas.focus(problem.nodeId ?? null, problem.edgeId ?? null);
        });
      }
      row.textContent = problem.text;
      root.appendChild(row);
    }
  }

  /** Debounce automatic validation so typing does not send a request per key. */
  private scheduleValidation(delay = 350): void {
    if (this.validationTimer !== null) {
      window.clearTimeout(this.validationTimer);
      this.validationTimer = null;
    }
    if (!this.connected) {
      this.setValidationState(
        "unavailable",
        t("validation.runtimeUnavailable"),
      );
      return;
    }
    this.setValidationState("checking", t("validation.checkingDetail"));
    this.validationTimer = window.setTimeout(() => {
      this.validationTimer = null;
      void this.validate(false);
    }, delay);
  }

  /**
   * Validate the workflow against the live runtime.
   *
   * Automatic passes stay quiet and update the toolbar badge; a manual pass or
   * a pre-run pass can also reveal the Problems tab and log the outcome.
   */
  private async validate(showProblems = false): Promise<boolean> {
    if (this.validationTimer !== null) {
      window.clearTimeout(this.validationTimer);
      this.validationTimer = null;
    }
    if (!this.connected) {
      this.setValidationState("unavailable", t("toolbar.runtimeUnreachable"));
      if (showProblems) this.showTab("problems");
      return false;
    }

    const revision = this.workflowRevision;
    const token = ++this.validateToken;
    this.setValidationState("checking", t("validation.checkingDetail"));
    try {
      const report = await this.client.validate(this.workflow);
      if (token !== this.validateToken || revision !== this.workflowRevision) return false;

      this.diagnostics = report.diagnostics;
      const errors = report.diagnostics.filter((item) => item.severity === "error").length;
      const warnings = report.diagnostics.filter((item) => item.severity === "warning").length;
      this.setValidationState(
        errors > 0 ? "invalid" : "valid",
        errors > 0
          ? `${errors} validation error(s), ${warnings} warning(s).`
          : `${warnings} validation warning(s).`,
        errors,
        warnings,
      );
      // Re-rendering the inspector while a field is focused would discard the
      // caret after the debounce; Problems still receives every diagnostic.
      if (!element("inspector").contains(document.activeElement)) {
        this.renderInspector();
      }
      this.renderProblems();
      if (showProblems) this.showTab("problems");
      if (showProblems) {
        this.pushLocal(
          errors === 0
            ? t("validation.manualValid", { count: report.diagnostics.length })
            : t("validation.manualInvalid", { errors }),
        );
      }
      return errors === 0;
    } catch (error) {
      if (token !== this.validateToken || revision !== this.workflowRevision) return false;
      if (!(error instanceof RuntimeError)) this.connected = false;
      this.diagnostics = [];
      this.setValidationState(
        "unavailable",
        error instanceof RuntimeError ? `${error.code}: ${error.message}` : String(error),
      );
      this.renderProblems();
      if (showProblems) this.reportError(error);
      return false;
    }
  }

  private async run(startPaused = false, stepImmediately = false): Promise<void> {
    if (this.runStarting) return;
    this.runStarting = true;
    this.updateRunControls();
    try {
      const valid = await this.validate(false);
      if (!valid) {
        this.showTab("problems");
        this.pushLocal(t("validation.runBlocked"));
        return;
      }

      this.closeStream?.();
      this.log.clear();
      this.canvas.clearStates();
      this.setStatus("pending");
      const snapshot = await this.client.createRun(
        this.workflow,
        Object.fromEntries(this.runOverrides),
        startPaused,
      );
      this.runId = snapshot.id;
      this.setStatus(snapshot.status);
      this.showTab("events");
      this.pushLocal(t("status.runAccepted", { id: snapshot.id }));

      // The socket close event fires asynchronously; by then this.runId may
      // already point at a newer run, and the old stream must not touch it.
      this.closeStream = this.client.streamRunEvents(snapshot.id, {
        onEvent: (envelope) => {
          this.log.append(envelope);
          this.applyEvent(envelope.event);
        },
        onClose: () => {
          if (this.runId === snapshot.id) void this.refreshRun();
          void this.refreshRunsIfVisible();
        },
      });
      if (stepImmediately) {
        const stepped = await this.client.step(snapshot.id);
        this.setStatus(stepped.status);
      }
    } catch (error) {
      this.setStatus(null);
      this.reportError(error);
    } finally {
      this.runStarting = false;
      this.updateRunControls();
    }
  }

  private applyEvent(event: { type: string; code?: string; message?: string }): void {
    switch (event.type) {
      case "run_started":
        this.setStatus("running");
        break;
      case "run_completed":
        this.setStatus("completed");
        break;
      case "run_paused":
        this.setStatus("paused");
        break;
      case "run_resumed":
        this.setStatus("running");
        break;
      case "run_cancelled":
        this.setStatus("cancelled");
        break;
      case "run_failed":
        this.setStatus("failed");
        this.diagnostics = [
          ...this.diagnostics,
          {
            severity: "error",
            code: event.code ?? "E_RUN",
            message: event.message ?? "run failed",
            path: "/runs",
          },
        ];
        this.renderProblems();
        break;
      default:
        break;
    }
  }

  /**
   * Step the active paused run, or start a new paused run and execute its first
   * node when idle. This makes single-step debugging useful even for very short
   * workflows that would otherwise finish before Pause can be clicked.
   */
  private async stepRun(): Promise<void> {
    if (this.stepStarting) return;
    if (
      this.currentRunStatus === "paused" ||
      this.currentRunStatus === "pending" ||
      this.currentRunStatus === "running"
    ) {
      if (this.currentRunStatus === "paused") {
        this.stepStarting = true;
        try {
          await this.control("step");
        } finally {
          this.stepStarting = false;
        }
      }
      return;
    }
    await this.run(true, true);
  }

  private async control(action: "pause" | "resume" | "step" | "cancel"): Promise<void> {
    if (!this.runId) return;
    try {
      const snapshot = await this.client[action](this.runId);
      this.setStatus(snapshot.status);
      void this.refreshRunsIfVisible();
    } catch (error) {
      this.reportError(error);
    }
  }

  /** Poll the runtime for agent sessions while the Agent tab is open. */
  private async pollAgentSessions(): Promise<void> {
    if (this.agentPoll !== null) {
      window.clearTimeout(this.agentPoll);
      this.agentPoll = null;
    }
    try {
      const list = await this.client.agentSessions();
      this.agents.setSessions(list);
      this.lastAgentPollError = null;
      if (AgentPanel.needsAttention(list)) {
        this.pulseAgentTab(true);
      }
    } catch (error) {
      // While the runtime is down this fires every tick; only report the
      // transition into failure, not every repeat of the same error.
      const message = error instanceof RuntimeError ? `${error.code}: ${error.message}` : String(error);
      if (message !== this.lastAgentPollError) {
        this.lastAgentPollError = message;
        this.reportError(error);
      }
    } finally {
      // Keep polling only while the tab is visible; a hidden tab costs nothing.
      const visible = !element("panel-agent").hidden;
      if (visible) {
        this.agentPoll = window.setTimeout(() => void this.pollAgentSessions(), 1500);
      } else {
        this.agentPoll = null;
        this.lastAgentPollError = null;
      }
    }
  }

  /** Mark the Agent tab when an approval is waiting, so it is not missed. */
  private pulseAgentTab(attention: boolean): void {
    const tab = document.querySelector<HTMLButtonElement>('.tab[data-tab="agent"]');
    if (tab) tab.classList.toggle("tab--attention", attention);
  }

  private async decideApproval(
    sessionId: string,
    approvalId: string,
    approve: boolean,
  ): Promise<void> {
    try {
      await this.client.decideApproval(
        sessionId,
        approvalId,
        approve ? "approved" : "denied",
      );
      this.pushLocal(t("status.approval", {
        decision: approve ? t("actions.approve") : t("actions.deny"),
        id: approvalId.slice(0, 8),
        session: sessionId.slice(0, 8),
      }));
      await this.pollAgentSessions();
    } catch (error) {
      this.reportError(error);
    }
  }

  /** Replace the document with the plan the agent proposed. */
  private loadPlan(sessionId: string): void {
    const session = this.agents.selectedSession();
    if (!session || session.id !== sessionId || !session.plan) {
      this.pushLocal(t("status.noPlan"));
      return;
    }
    if (!this.replaceWorkflow(session.plan.workflow)) return;
    this.showTab("json");
    this.pushLocal(t("status.planLoaded", { id: sessionId.slice(0, 8) }));
  }

  /** Follow the run a session started. */
  private async openRun(runId: string): Promise<void> {
    this.runId = runId;
    this.closeStream?.();
    this.log.clear();
    this.showTab("events");
    try {
      const snapshot = await this.client.getRun(runId);
      this.setStatus(snapshot.status);
      this.closeStream = this.client.streamRunEvents(runId, {
        onEvent: (envelope) => {
          this.log.append(envelope);
          this.applyEvent(envelope.event);
        },
        onClose: () => {
          if (this.runId === runId) void this.refreshRun();
          void this.refreshRunsIfVisible();
        },
      });
    } catch (error) {
      this.reportError(error);
    }
  }

  /** Load the audit log, optionally narrowed to the run being watched. */
  private async refreshAudit(): Promise<void> {
    const onlyCurrent = element<HTMLInputElement>("audit-current-run").checked;
    // Sequence requests so an out-of-order response cannot show stale records.
    const token = ++this.auditToken;
    try {
      const records = await this.client.audit({
        runId: onlyCurrent && this.runId ? this.runId : undefined,
        limit: 500,
      });
      if (token !== this.auditToken) return;
      this.audit.setRecords(records);
    } catch (error) {
      this.reportError(error);
    }
  }

  /** Load unified built-in, in-process and plugin registration metadata. */
  private async refreshExtensions(): Promise<void> {
    const token = ++this.extensionsToken;
    try {
      const extensions = await this.client.extensions();
      if (token !== this.extensionsToken) return;
      this.extensionsPanel.setExtensions(extensions);
    } catch (error) {
      if (token === this.extensionsToken) this.reportError(error);
    }
  }

  /** Load the runtime's run catalogue for the Runs tab. */
  private async refreshRuns(): Promise<void> {
    const token = ++this.runsToken;
    try {
      const runs = await this.client.listRuns();
      if (token !== this.runsToken) return;
      this.runsPanel.setRuns(runs);
    } catch (error) {
      if (token === this.runsToken) this.reportError(error);
    }
  }

  private async refreshRunsIfVisible(): Promise<void> {
    if (!element("panel-runs").hidden) await this.refreshRuns();
  }

  private async refreshRun(): Promise<void> {
    if (!this.runId) return;
    try {
      const snapshot = await this.client.getRun(this.runId);
      this.setStatus(snapshot.status);
      if (snapshot.failure) {
        this.pushLocal(t("status.runFailure", {
          code: snapshot.failure.code,
          message: snapshot.failure.message,
        }));
      }
    } catch {
      // The run may have been discarded; the viewer stays usable either way.
    }
  }

  private setStatus(status: RunStatus | null): void {
    this.currentRunStatus = status;
    this.log.setRunStatus(status);
    this.updateRunControls();
  }

  private setValidationState(
    state: ValidationState,
    detail: string,
    errors = 0,
    warnings = 0,
  ): void {
    this.validationState = state;
    const badge = element<HTMLButtonElement>("validation-status");
    badge.dataset.state = state;
    switch (state) {
      case "checking":
        badge.textContent = t("validation.checking");
        break;
      case "valid":
        badge.textContent = warnings > 0 ? t("validation.validWarnings", { warnings }) : t("validation.valid");
        break;
      case "invalid":
        badge.textContent = warnings > 0 ? t("validation.invalidWarnings", { errors, warnings }) : t("validation.invalid", { errors });
        break;
      case "unavailable":
        badge.textContent = t("validation.unavailable");
        break;
      default:
        badge.textContent = t("validation.notChecked");
        break;
    }
    badge.title = detail;
    this.updateRunControls();
  }

  private updateRunControls(): void {
    const localErrorCount = localProblems(this.workflow).length;
    const controls = deriveRunControls(
      this.currentRunStatus,
      this.connected,
      this.validationState,
      localErrorCount,
      this.runStarting,
    );
    element<HTMLButtonElement>("btn-run").disabled = controls.runDisabled;
    element<HTMLButtonElement>("btn-run-options").disabled = controls.runDisabled;
    element<HTMLButtonElement>("btn-pause").disabled = controls.pauseDisabled;
    element<HTMLButtonElement>("btn-resume").disabled = controls.resumeDisabled;
    element<HTMLButtonElement>("btn-step").disabled = controls.stepDisabled;
    element<HTMLButtonElement>("btn-cancel").disabled = controls.cancelDisabled;
    element<HTMLButtonElement>("btn-validate").disabled = !this.connected;
  }

  private reportError(error: unknown): void {
    if (error instanceof RuntimeError) {
      this.pushLocal(`${error.code}: ${error.message}`);
    } else {
      this.pushLocal((error as Error).message ?? String(error));
    }
  }

  private pushLocal(message: string): void {
    const row = document.createElement("div");
    row.className = "event event--local";
    const time = document.createElement("span");
    time.className = "event__time";
    time.textContent = new Date().toLocaleTimeString();
    const kind = document.createElement("span");
    kind.className = "event__kind";
    kind.textContent = t("event.studio");
    const body = document.createElement("span");
    body.className = "event__body";
    body.textContent = message;
    row.append(time, kind, body);
    element("events").appendChild(row);
  }
}

const studio = new Studio();
void studio.start();
