/**
 * Studio entry point.
 *
 * Wiring only: connect, discover, render, execute. Everything with real logic
 * lives in `runtime/`, `model/` or `ui/`.
 */

import "./styles.css";

import {
  applyWorkflow,
  emptyWorkflow,
  localProblems,
  WORKFLOW_SCHEMA_PATH,
} from "./model/workflow";
import { RuntimeClient, RuntimeError } from "./runtime/client";
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
import { Inspector } from "./ui/inspector";
import { Palette } from "./ui/palette";

function element<T extends Element = HTMLElement>(id: string): T {
  const found = document.getElementById(id);
  if (!found) throw new Error(`missing element #${id}`);
  return found as unknown as T;
}

class Studio {
  private readonly client = new RuntimeClient();

  /**
   * The editor mutates this object in place rather than replacing it, because
   * the canvas, inspector and event log all hold a reference to it.
   */
  private readonly workflow: Workflow = emptyWorkflow();

  private descriptors = new Map<string, NodeDescriptor>();
  private diagnostics: Diagnostic[] = [];
  private runId: string | null = null;
  private closeStream: (() => void) | null = null;

  private readonly palette: Palette;
  private readonly canvas: Canvas;
  private readonly inspector: Inspector;
  private readonly log: EventLog;
  private readonly agents: AgentPanel;
  private readonly audit: AuditPanel;
  private agentPoll: number | null = null;

  constructor() {
    const canvasElement = element<SVGSVGElement>("canvas");

    this.canvas = new Canvas(canvasElement, this.workflow, {
      onChange: () => this.refresh(),
      onSelect: (nodeId) => this.inspector.render(nodeId, this.diagnostics),
      onStatus: (message) => this.pushLocal(message),
      descriptorFor: (nodeType) => this.descriptors.get(nodeType),
    });
    // The canvas must exist before the palette can call back into it, so the
    // palette is constructed with a lazy reference rather than a captured value.
    this.palette = new Palette(element("palette"), {
      onAdd: (descriptor) =>
        this.canvas.addNode(
          descriptor,
          120 + this.workflow.nodes.length * 24,
          120 + this.workflow.nodes.length * 24,
        ),
    });
    this.inspector = new Inspector(
      element("inspector"),
      this.workflow,
      (nodeType) => this.descriptors.get(nodeType),
      { onChange: () => this.refresh() },
    );
    this.log = new EventLog(element("events"), this.canvas);
    this.agents = new AgentPanel(element("agent"), {
      onDecide: (sessionId, approvalId, approve) =>
        void this.decideApproval(sessionId, approvalId, approve),
      onLoadPlan: (sessionId) => this.loadPlan(sessionId),
      onOpenRun: (runId) => void this.openRun(runId),
    });
    this.audit = new AuditPanel(element("audit"));

    this.bindToolbar();
    this.refresh();
    this.setStatus("pending");
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
      const descriptors = await this.client.nodeTypes();
      this.descriptors = new Map(descriptors.map((descriptor) => [descriptor.node_type, descriptor]));
      this.palette.setDescriptors(descriptors);

      const failed = plugins.failures.length;
      badge.textContent = `${health.node_types} node types · ${plugins.plugins.length} plugin(s)${failed ? ` · ${failed} failed` : ""}`;
      badge.className = `connection connection--${failed ? "warn" : "ok"}`;
      badge.title = plugins.failures
        .map((failure) => `${failure.id}: ${failure.message}`)
        .join("\n");
      if (quiet) this.refresh();
    } catch (error) {
      badge.textContent = "runtime unreachable";
      badge.className = "connection connection--error";
      badge.title = (error as Error).message;
    }
  }

  private bindToolbar(): void {
    element("btn-new").addEventListener("click", () => {
      if (!confirm("Discard the current workflow?")) return;
      this.replaceWorkflow(emptyWorkflow());
    });

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
        this.pushLocal(`could not import: ${(error as Error).message}`);
      } finally {
        fileInput.value = "";
      }
    });

    element("btn-validate").addEventListener("click", () => void this.validate());
    element("btn-run").addEventListener("click", () => void this.run());
    element("btn-pause").addEventListener("click", () => void this.control("pause"));
    element("btn-resume").addEventListener("click", () => void this.control("resume"));
    element("btn-step").addEventListener("click", () => void this.control("step"));
    element("btn-cancel").addEventListener("click", () => void this.control("cancel"));

    element<HTMLInputElement>("palette-filter").addEventListener("input", (event) => {
      this.palette.filter((event.target as HTMLInputElement).value);
    });

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

    element("btn-apply-json").addEventListener("click", () => {
      try {
        this.replaceWorkflow(
          JSON.parse(element<HTMLTextAreaElement>("json-view").value) as Partial<Workflow>,
        );
        this.pushLocal("workflow replaced from JSON");
      } catch (error) {
        this.pushLocal(`invalid workflow JSON: ${(error as Error).message}`);
      }
    });

    element("btn-runtime-schema").addEventListener("click", () => {
      this.workflow.$schema = this.runtimeSchemaUrl();
      this.refresh();
      this.showTab("json");
      this.pushLocal(`$schema now points at ${this.workflow.$schema}`);
    });
  }

  /**
   * The schema URL this deployment serves, which completes exactly the node
   * types the connected runtime installed rather than a static file.
   */
  private runtimeSchemaUrl(): string {
    return `${window.location.origin}/api/v1/schema/workflow`;
  }

  private replaceWorkflow(next: Partial<Workflow> | null): void {
    applyWorkflow(this.workflow, next);
    this.diagnostics = [];
    this.canvas.select(null);
    this.refresh();
  }

  private showTab(name: string): void {
    for (const tab of document.querySelectorAll<HTMLButtonElement>(".tab")) {
      tab.classList.toggle("tab--active", tab.dataset.tab === name);
    }
    for (const panel of document.querySelectorAll<HTMLElement>(".drawer__panel")) {
      panel.hidden = panel.id !== `panel-${name}`;
    }
    if (name === "json") this.renderJson();
  }

  private refresh(): void {
    this.canvas.render();
    this.renderJson();
    this.renderProblems();
  }

  private renderJson(): void {
    element<HTMLTextAreaElement>("json-view").value = JSON.stringify(this.workflow, null, 2);
    const hint = element("json-schema");
    const declared = this.workflow.$schema || WORKFLOW_SCHEMA_PATH;
    const origin = declared === WORKFLOW_SCHEMA_PATH ? "the published schema" : declared;
    hint.textContent = `$schema: ${origin} — ${this.descriptors.size || "no"} node type(s) available to complete`;
  }

  private renderProblems(): void {
    const root = element("problems");
    root.replaceChildren();
    const problems = [
      ...localProblems(this.workflow).map((text) => ({ severity: "error", text })),
      ...this.diagnostics.map((diagnostic) => ({
        severity: diagnostic.severity,
        text: `[${diagnostic.code}] ${diagnostic.message} (${diagnostic.path})${
          diagnostic.hint ? ` — ${diagnostic.hint}` : ""
        }`,
      })),
    ];
    if (problems.length === 0) {
      const ok = document.createElement("p");
      ok.className = "muted";
      ok.textContent = "No problems detected.";
      root.appendChild(ok);
      return;
    }
    for (const problem of problems) {
      const row = document.createElement("p");
      row.className = `problem problem--${problem.severity}`;
      row.textContent = problem.text;
      root.appendChild(row);
    }
  }

  private async validate(): Promise<void> {
    try {
      const report = await this.client.validate(this.workflow);
      this.diagnostics = report.diagnostics;
      this.showTab("problems");
      this.inspector.render(this.canvas.selectedNodeId(), this.diagnostics);
      this.renderProblems();
      const errors = report.diagnostics.filter((item) => item.severity === "error").length;
      this.pushLocal(
        errors === 0
          ? `valid — ${report.diagnostics.length} diagnostic(s)`
          : `invalid — ${errors} error(s)`,
      );
    } catch (error) {
      this.reportError(error);
    }
  }

  private async run(): Promise<void> {
    try {
      this.closeStream?.();
      this.log.clear();
      this.canvas.clearStates();
      this.setStatus("pending");
      const snapshot = await this.client.createRun(this.workflow);
      this.runId = snapshot.id;
      this.setStatus(snapshot.status);
      this.showTab("events");
      this.pushLocal(`run ${snapshot.id} accepted`);

      this.closeStream = this.client.streamRunEvents(snapshot.id, {
        onEvent: (envelope) => {
          this.log.append(envelope);
          this.applyEvent(envelope.event);
        },
        onClose: () => void this.refreshRun(),
      });
    } catch (error) {
      this.reportError(error);
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

  private async control(action: "pause" | "resume" | "step" | "cancel"): Promise<void> {
    if (!this.runId) return;
    try {
      const snapshot = await this.client[action](this.runId);
      this.setStatus(snapshot.status);
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
      if (AgentPanel.needsAttention(list)) {
        this.pulseAgentTab(true);
      }
    } catch (error) {
      this.reportError(error);
    } finally {
      // Keep polling only while the tab is visible; a hidden tab costs nothing.
      const visible = !element("panel-agent").hidden;
      if (visible) {
        this.agentPoll = window.setTimeout(() => void this.pollAgentSessions(), 1500);
      } else {
        this.agentPoll = null;
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
      this.pushLocal(
        `${approve ? "approved" : "denied"} ${approvalId.slice(0, 8)}… in session ${sessionId.slice(0, 8)}…`,
      );
      await this.pollAgentSessions();
    } catch (error) {
      this.reportError(error);
    }
  }

  /** Replace the document with the plan the agent proposed. */
  private loadPlan(sessionId: string): void {
    const session = this.agents.selectedSession();
    if (!session || session.id !== sessionId || !session.plan) {
      this.pushLocal("that session has no plan to load");
      return;
    }
    this.replaceWorkflow(session.plan.workflow);
    this.showTab("json");
    this.pushLocal(`loaded plan from session ${sessionId.slice(0, 8)}…`);
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
        onClose: () => void this.refreshRun(),
      });
    } catch (error) {
      this.reportError(error);
    }
  }

  /** Load the audit log, optionally narrowed to the run being watched. */
  private async refreshAudit(): Promise<void> {
    const onlyCurrent = element<HTMLInputElement>("audit-current-run").checked;
    try {
      const records = await this.client.audit({
        runId: onlyCurrent && this.runId ? this.runId : undefined,
        limit: 500,
      });
      this.audit.setRecords(records);
    } catch (error) {
      this.reportError(error);
    }
  }

  private async refreshRun(): Promise<void> {
    if (!this.runId) return;
    try {
      const snapshot = await this.client.getRun(this.runId);
      this.setStatus(snapshot.status);
      if (snapshot.failure) {
        this.pushLocal(`failure [${snapshot.failure.code}] ${snapshot.failure.message}`);
      }
    } catch {
      // The run may have been discarded; the viewer stays usable either way.
    }
  }

  private setStatus(status: RunStatus): void {
    const inFlight = status === "running" || status === "paused" || status === "pending";
    const paused = status === "paused";
    element<HTMLButtonElement>("btn-run").disabled = inFlight;
    element<HTMLButtonElement>("btn-pause").disabled = !inFlight || paused;
    element<HTMLButtonElement>("btn-resume").disabled = !paused;
    element<HTMLButtonElement>("btn-step").disabled = !paused;
    element<HTMLButtonElement>("btn-cancel").disabled = !inFlight;
    this.log.setRunStatus(status);
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
    kind.textContent = "studio";
    const body = document.createElement("span");
    body.className = "event__body";
    body.textContent = message;
    row.append(time, kind, body);
    element("events").appendChild(row);
  }
}

const studio = new Studio();
void studio.start();
