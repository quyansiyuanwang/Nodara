/**
 * The conversational Agent panel.
 *
 * Runtime sessions are the source of truth. The desktop shell starts a
 * `nodara-agent studio` process for each turn, while session polling keeps the
 * transcript, plan, approvals, run and audit link live.
 */

import { localizeAgentStatus, localizeDiagnostic, t } from "../i18n";
import {
  AgentBaseMode,
  AgentMode,
  AgentProviderProfile,
  AgentSettings,
  allTemplates,
  defaultAgentSettings,
  findTemplate,
  isBuiltinTemplate,
  loadAgentSettings,
  saveAgentSettings,
} from "../model/agent-settings";
import { diffWorkflows, isWorkflowDiffEmpty, WorkflowDiff } from "../model/workflow-diff";
import {
  AgentSession,
  AgentSessionList,
  ApprovalRequest,
  ArtifactMeta,
  ValidationReport,
  Workflow,
} from "../runtime/types";
import { AgentSettingsDialog } from "./agent-settings";

export type AgentExecutionMode = AgentMode;

export interface AgentProviderConfig extends AgentProviderProfile {
  profileId: string;
  apiKey: string;
}

export interface AgentSubmitRequest {
  goal: string;
  mode: AgentExecutionMode;
  baseMode: AgentBaseMode;
  provider: AgentProviderConfig;
  sessionId?: string;
  extraInstructions: string;
  promptTemplateId?: string;
  promptTemplateInstructions?: string;
}

export interface AgentTurnResponse {
  session_id?: string;
  accepted: boolean;
  workflow?: Workflow;
  run?: { id: string; status: string };
  report?: unknown;
  trace?: unknown[];
  tokens_used?: number;
}

export type AgentStreamEvent =
  | { type: "turn_started" }
  | { type: "model_delta"; text: string }
  | { type: "phase_changed"; phase: string; message: string }
  | { type: "validation_result"; accepted: boolean; errors: number; warnings: number }
  | { type: "repair_started"; attempt: number }
  | { type: "plan_ready"; workflow: Workflow }
  | { type: "run_finished"; run: unknown }
  | { type: "completed"; response: AgentTurnResponse }
  | { type: "failed"; message: string }
  | { type: "cancelled" };

interface TraceLine { type: string; label: string; detail?: string }

/**
 * What each execution mode actually does.
 *
 * A bare "Partial approval" in a dropdown does not tell an operator whether the
 * turn will run at all, so the row states the consequence underneath it.
 */
const MODE_HINT_KEYS: Record<AgentMode, string> = {
  forbidden: "agent.modeHint.forbidden",
  manual: "agent.modeHint.manual",
  partial: "agent.modeHint.partial",
  all: "agent.modeHint.all",
};

interface AgentFocusSnapshot {
  field: string;
  start: number | null;
  end: number | null;
}

interface AgentScrollSnapshot {
  main: number;
  sessions: number;
}

export interface AgentPanelHandlers {
  onSubmit: (
    request: AgentSubmitRequest,
    turnId: string,
    onEvent: (event: AgentStreamEvent) => void,
  ) => Promise<void>;
  onStopGeneration: (turnId: string) => Promise<boolean>;
  onCredentialGet: (profileId: string) => Promise<string | null>;
  onCredentialSet: (profileId: string, secret: string) => Promise<void>;
  onCredentialDelete: (profileId: string) => Promise<void>;
  getCurrentWorkflow: () => Workflow;
  onDecide: (sessionId: string, approvalId: string, approve: boolean) => void;
  onLoadPlan: (sessionId: string) => void;
  onOpenRun: (runId: string) => void;
  onOpenAudit: (runId: string) => void;
  onResumeRun: (runId: string) => void;
  onValidatePlan: (workflow: Workflow) => Promise<ValidationReport>;
  onRunPlan: (workflow: Workflow, sessionId: string, mode: AgentExecutionMode) => Promise<void>;
  /** Capture the screen and bind the run to the session the next turn continues. */
  onObserveScreen: (sessionId: string, mode: AgentExecutionMode) => Promise<void>;
  /** Image and non-image artifacts a run produced, for the evidence chip. */
  listArtifacts: (runId: string) => Promise<ArtifactMeta[]>;
  artifactUrl: (runId: string, artifactId: string) => string;
}

export class AgentPanel {
  private sessions: AgentSession[] = [];
  private selected: string | null = null;
  private draft = "";
  private busy = false;
  private localError = "";
  private settings: AgentSettings = loadAgentSettings();
  private readonly settingsDialog: AgentSettingsDialog;
  private readonly planJsonOpen = new Set<string>();
  private readonly traceOpen = new Set<string>();
  private readonly sessionTraces = new Map<string, unknown[]>();
  private readonly evidence = new Map<string, ArtifactMeta[]>();
  private templateId = "";
  private sessionFingerprint = "";
  private sessionFilter = "";
  private streamPhase = "";
  private streamText = "";
  private streamWorkflow: Workflow | null = null;
  private streamTrace: TraceLine[] = [];
  private activeTurnId: string | null = null;
  private workspaceOpen = this.settings.workspaceOpen;
  private streamFrame: number | null = null;

  constructor(
    private readonly root: HTMLElement,
    private readonly handlers: AgentPanelHandlers,
    private readonly desktopAvailable = true,
  ) {
    // The expanded workspace covers the viewport, so the escape hatch listens on
    // the document rather than on a panel that may not currently hold focus.
    document.addEventListener("keydown", (event) => this.onKeyDown(event));
    this.settingsDialog = new AgentSettingsDialog(
      {
        onChange: () => {
          this.persist();
          // Keep the toolbar chip in step with the dialog. The panel's own focus
          // restore ignores the dialog, so nothing the operator is typing moves.
          this.render();
        },
        onCredentialGet: (profileId) => this.handlers.onCredentialGet(profileId),
        onCredentialSet: (profileId, secret) => this.handlers.onCredentialSet(profileId, secret),
        onCredentialDelete: (profileId) => this.handlers.onCredentialDelete(profileId),
      },
      this.desktopAvailable,
    );
    this.render();
    void this.settingsDialog.refreshCredential(this.settings).then(() => this.render());
  }

  /** Open the configuration dialog. */
  openSettings(): void {
    this.settingsDialog.open(this.settings);
  }

  /** Expand or collapse the full workspace, persisting the choice. */
  private setWorkspaceOpen(open: boolean): void {
    this.workspaceOpen = open;
    this.persist();
    this.render();
  }

  /** Escape leaves the expanded workspace, like every other overlay here. */
  private onKeyDown(event: KeyboardEvent): void {
    if (event.key !== "Escape" || !this.workspaceOpen) return;
    if (document.querySelector("dialog[open]")) return;
    event.stopPropagation();
    this.setWorkspaceOpen(false);
  }

  /** Replace the session list. Preserves the current selection when possible. */
  setSessions(list: AgentSessionList): void {
    const fingerprint = JSON.stringify({
      sessions: list.sessions,
      pendingApprovals: list.pending_approvals,
    });
    if (fingerprint === this.sessionFingerprint) return;
    this.sessionFingerprint = fingerprint;
    this.sessions = list.sessions;
    const pending = list.pending_approvals;
    if (this.selected && !this.sessions.some((session) => session.id === this.selected)) {
      this.selected = null;
    }
    if (!this.selected) {
      this.selected = pending[0]?.session_id ?? this.sessions[0]?.id ?? null;
    }
    this.render();
  }

  /** Session currently shown, if any. */
  selectedSession(): AgentSession | undefined {
    return this.sessions.find((session) => session.id === this.selected);
  }

  /** Focus a session after a desktop Agent turn creates or reuses it. */
  selectSession(sessionId: string | null): void {
    this.selected = sessionId;
    this.render();
  }

  /** Keep the durable Agent trace attached to its visible session. */
  setSessionTrace(sessionId: string, trace: unknown[]): void {
    this.sessionTraces.set(sessionId, trace);
    if (sessionId === this.selected) this.render();
  }

  /** True when at least one approval is blocking a run. */
  static needsAttention(list: AgentSessionList): boolean {
    return list.pending_approvals.length > 0;
  }

  private activeProfile(): AgentProviderProfile {
    return this.settings.profiles.find((profile) => profile.id === this.settings.activeProfileId)
      ?? this.settings.profiles[0]
      ?? defaultAgentSettings().profiles[0];
  }

  private providerConfig(): AgentProviderConfig {
    const profile = this.activeProfile();
    return { ...profile, profileId: profile.id, apiKey: this.settingsDialog.activeApiKey() };
  }

  private persist(): void {
    this.settings.workspaceOpen = this.workspaceOpen;
    saveAgentSettings(this.settings);
  }

  /**
   * Load the artifacts the selected session's last run produced.
   *
   * Those are exactly the images the next turn will receive, so the panel can
   * show the observation loop instead of leaving it invisible.
   */
  private async loadEvidence(session: AgentSession | undefined): Promise<void> {
    const runId = session?.run_id;
    if (!runId || this.evidence.has(runId) || !this.desktopAvailable) return;
    try {
      const artifacts = await this.handlers.listArtifacts(runId);
      this.evidence.set(runId, artifacts);
      if (session?.id === this.selected) this.render();
    } catch {
      // Evidence is an affordance, not a requirement: the turn still works.
    }
  }

  private captureFocus(): AgentFocusSnapshot | null {
    const active = document.activeElement;
    if (!(active instanceof HTMLElement) || !this.root.contains(active)) return null;
    const field = active.dataset.agentFocus;
    if (!field) return null;
    const editable =
      active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement;
    return {
      field,
      start: editable ? active.selectionStart : null,
      end: editable ? active.selectionEnd : null,
    };
  }

  private restoreFocus(focus: AgentFocusSnapshot | null): void {
    if (!focus) return;
    const target = this.root.querySelector<HTMLElement>(
      `[data-agent-focus="${focus.field}"]`,
    );
    if (!target) return;
    target.focus();
    if (
      focus.start === null ||
      focus.end === null ||
      !(target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement)
    ) {
      return;
    }
    try {
      target.setSelectionRange(focus.start, focus.end);
    } catch {
      // Number inputs do not expose a text selection range.
    }
  }

  private captureScroll(): AgentScrollSnapshot {
    return {
      main: this.root.querySelector<HTMLElement>(".agent-main")?.scrollTop ?? 0,
      sessions: this.root.querySelector<HTMLElement>(".sessions")?.scrollTop ?? 0,
    };
  }

  private restoreScroll(scroll: AgentScrollSnapshot): void {
    const main = this.root.querySelector<HTMLElement>(".agent-main");
    const sessions = this.root.querySelector<HTMLElement>(".sessions");
    if (main) main.scrollTop = scroll.main;
    if (sessions) sessions.scrollTop = scroll.sessions;
  }

  private render(): void {
    const focus = this.captureFocus();
    const scroll = this.captureScroll();
    for (const details of this.root.querySelectorAll<HTMLDetailsElement>(".agent-plan-json")) {
      const sessionId = details.dataset.sessionId;
      if (!sessionId) continue;
      if (details.open) this.planJsonOpen.add(sessionId);
      else this.planJsonOpen.delete(sessionId);
    }

    const draft = this.draft;
    this.root.replaceChildren();
    this.root.classList.toggle("agent--workspace", this.workspaceOpen);
    document.body.classList.toggle("agent-workspace-open", this.workspaceOpen);
    const shell = document.createElement("div");
    shell.className = "agent-shell";

    const sidebar = document.createElement("div");
    sidebar.className = "agent-sidebar";
    const sidebarHeader = document.createElement("div");
    sidebarHeader.className = "agent-sidebar__header";
    const sessionCount = document.createElement("span");
    sessionCount.className = "agent-sidebar__count";
    sessionCount.textContent = t("agent.sessionCount", { count: this.sessions.length });
    const newChat = document.createElement("button");
    newChat.type = "button";
    newChat.className = "btn btn--small";
    newChat.dataset.agentFocus = "new-chat";
    newChat.textContent = t("agent.newChat");
    newChat.addEventListener("click", () => {
      this.selected = null;
      this.draft = "";
      this.localError = "";
      this.render();
    });
    sidebarHeader.append(sessionCount, newChat);
    sidebar.appendChild(sidebarHeader);

    const search = document.createElement("input");
    search.className = "input input--small agent-search";
    search.type = "search";
    search.dataset.agentFocus = "session-search";
    search.placeholder = t("agent.searchSessions");
    search.value = this.sessionFilter;
    sidebar.appendChild(search);
    const list = document.createElement("div");
    list.className = "sessions";
    const renderSessions = () => this.renderSessionItems(list);
    search.addEventListener("input", () => {
      this.sessionFilter = search.value;
      renderSessions();
    });
    renderSessions();
    sidebar.appendChild(list);
    shell.appendChild(sidebar);

    const main = document.createElement("div");
    main.className = "agent-main";
    main.appendChild(this.renderToolbar());
    if (!this.desktopAvailable) {
      const desktop = document.createElement("p");
      desktop.className = "gate agent-desktop-only";
      desktop.textContent = t("agent.desktopOnly");
      main.appendChild(desktop);
    }
    const session = this.selectedSession();
    main.appendChild(this.renderControls(session));
    if (this.busy || this.streamText || this.streamTrace.length > 0) {
      main.appendChild(this.renderLiveTurn());
    }
    if (session) main.appendChild(this.renderDetail(session));
    if (this.localError) {
      const error = document.createElement("p");
      error.className = "problem problem--error";
      error.textContent = this.localError;
      main.appendChild(error);
    }
    main.appendChild(this.renderComposer(draft));
    shell.appendChild(main);
    this.root.appendChild(shell);
    this.restoreScroll(scroll);
    this.restoreFocus(focus);
    // The evidence chip needs the run's artifacts, fetched once per run.
    void this.loadEvidence(session);
  }

  /** Provider summary, settings entry point and the workspace toggle. */
  private renderToolbar(): HTMLElement {
    const toolbar = document.createElement("div");
    toolbar.className = "agent-workspace-header";
    const profile = this.activeProfile();
    const chip = document.createElement("button");
    chip.type = "button";
    chip.className = "agent-provider-chip";
    chip.dataset.agentFocus = "provider-summary";
    chip.title = t("agent.providerSettings");
    const chipName = document.createElement("strong");
    chipName.textContent = profile.name;
    const chipModel = document.createElement("code");
    chipModel.textContent = profile.model;
    chip.append(chipName, chipModel);
    chip.addEventListener("click", () => this.openSettings());

    const actions = document.createElement("div");
    actions.className = "agent-workspace-header__actions";
    const settings = document.createElement("button");
    settings.type = "button";
    settings.className = "btn btn--small";
    settings.dataset.agentFocus = "open-settings";
    settings.textContent = t("agent.openSettings");
    settings.addEventListener("click", () => this.openSettings());
    const workspaceToggle = document.createElement("button");
    workspaceToggle.type = "button";
    workspaceToggle.className = "btn btn--small";
    workspaceToggle.dataset.agentFocus = "workspace-toggle";
    workspaceToggle.textContent = this.workspaceOpen
      ? t("agent.collapseWorkspace")
      : t("agent.expandWorkspace");
    workspaceToggle.addEventListener("click", () => this.setWorkspaceOpen(!this.workspaceOpen));
    actions.append(settings, workspaceToggle);
    toolbar.append(chip, actions);
    return toolbar;
  }

  private renderSessionItems(list: HTMLElement): void {
    list.replaceChildren();
    const query = this.sessionFilter.trim().toLocaleLowerCase();
    const sessions = query
      ? this.sessions.filter((session) => session.goal.toLocaleLowerCase().includes(query))
      : this.sessions;
    for (const session of sessions) {
      const item = document.createElement("button");
      item.type = "button";
      item.className = "session";
      item.dataset.agentFocus = `session.${session.id}`;
      if (session.id === this.selected) item.classList.add("session--selected");
      if (session.approvals.some((approval) => !approval.decision)) {
        item.classList.add("session--attention");
      }
      const goal = document.createElement("span");
      goal.className = "session__goal";
      goal.textContent = session.goal;
      const status = document.createElement("span");
      status.className = `session__status session__status--${session.status}`;
      status.textContent = localizeAgentStatus(session.status);
      item.append(goal, status);
      item.addEventListener("click", () => {
        this.selected = session.id;
        this.render();
      });
      list.appendChild(item);
    }
    if (sessions.length === 0) {
      const empty = document.createElement("p");
      empty.className = "muted";
      empty.textContent = query ? t("agent.noSessionsMatch") : t("agent.empty");
      list.appendChild(empty);
    }
  }

  /**
   * Per-turn controls: execution mode, baseline, prompt template and the
   * observation entry point.
   *
   * Configuration lives in the settings dialog, so this row stays short enough
   * to sit above the composer even in a docked drawer, and the primary task —
   * describing a goal — is never pushed off screen by a form.
   */
  private renderControls(session: AgentSession | undefined): HTMLElement {
    const controls = document.createElement("div");
    controls.className = "agent-controls";

    const hint = document.createElement("p");
    hint.className = "agent-mode-hint";
    hint.dataset.agentFocus = "mode-hint";
    hint.textContent = this.modeHint(session);

    const mode = document.createElement("select");
    mode.className = "input input--small";
    mode.dataset.agentFocus = "execution-mode";
    for (const [value, key] of [
      ["forbidden", "agent.modeForbidden"],
      ["manual", "agent.modeManual"],
      ["partial", "agent.modePartial"],
      ["all", "agent.modeAll"],
    ] as const) {
      const option = document.createElement("option");
      option.value = value;
      option.textContent = t(key);
      mode.appendChild(option);
    }
    mode.value = this.settings.mode;
    mode.addEventListener("change", () => {
      this.settings.mode = mode.value as AgentExecutionMode;
      this.persist();
      hint.textContent = this.modeHint(this.selectedSession());
    });
    controls.appendChild(this.labeledControl("agent.modeLabel", mode));

    const base = document.createElement("select");
    base.className = "input input--small";
    base.dataset.agentFocus = "baseline-mode";
    base.setAttribute("aria-label", t("agent.baseLabel"));
    for (const [value, key] of [
      ["current", "agent.baseCurrent"],
      ["last_plan", "agent.baseLastPlan"],
    ] as const) {
      const option = document.createElement("option");
      option.value = value;
      option.textContent = t(key);
      base.appendChild(option);
    }
    base.value = this.settings.baseMode;
    // A disabled select explains itself instead of just refusing to open.
    base.disabled = !session?.plan;
    base.title = session?.plan ? "" : t("agent.lastPlanUnavailable");
    base.addEventListener("change", () => {
      this.settings.baseMode = base.value as AgentBaseMode;
      this.persist();
    });
    controls.appendChild(this.labeledControl("agent.baseLabel", base));

    const template = document.createElement("select");
    template.className = "input input--small";
    template.dataset.agentFocus = "prompt-template";
    template.setAttribute("aria-label", t("agent.templateLabel"));
    const none = document.createElement("option");
    none.value = "";
    none.textContent = t("agent.noTemplate");
    template.appendChild(none);
    for (const item of allTemplates(this.settings.templates)) {
      const option = document.createElement("option");
      option.value = item.id;
      option.textContent = isBuiltinTemplate(item.id)
        ? `${item.name} · ${t("agent.builtinTemplate")}`
        : item.name;
      template.appendChild(option);
    }
    // A template that no longer exists must not stay selected and silently send
    // nothing.
    if (!findTemplate(this.templateId, this.settings.templates)) this.templateId = "";
    template.value = this.templateId;
    template.addEventListener("change", () => {
      this.templateId = template.value;
      hint.textContent = this.modeHint(this.selectedSession());
    });
    controls.appendChild(this.labeledControl("agent.templateLabel", template));

    const observe = document.createElement("button");
    observe.type = "button";
    observe.className = "btn btn--small";
    observe.dataset.agentFocus = "observe-screen";
    observe.textContent = t("agent.observeScreen");
    // Observing *is* an execution: a plan-only mode must not run it, and the
    // button says why instead of doing nothing.
    const planOnly = this.settings.mode === "forbidden";
    observe.title = planOnly ? t("agent.runForbidden") : t("agent.observeHint");
    observe.disabled = !this.desktopAvailable || !session || planOnly;
    observe.addEventListener("click", () => {
      const current = this.selectedSession();
      if (current) void this.handlers.onObserveScreen(current.id, this.settings.mode);
    });
    controls.appendChild(observe);

    controls.appendChild(hint);
    const evidence = this.renderEvidence(session);
    if (evidence) controls.appendChild(evidence);
    return controls;
  }

  /** What the chosen execution mode will actually do, plus this turn's input. */
  private modeHint(session: AgentSession | undefined): string {
    const mode = t(MODE_HINT_KEYS[this.settings.mode]);
    const constraints = t("agent.constraintCount", { count: this.constraintCount() });
    return session
      ? `${mode} · ${constraints}`
      : `${mode} · ${constraints} · ${t("agent.observeFirstHint")}`;
  }

  /** How many operator constraints the next turn will carry. */
  private constraintCount(): number {
    const template = findTemplate(this.templateId, this.settings.templates);
    return (this.settings.extraInstructions.trim() ? 1 : 0)
      + (template?.instructions.trim() ? 1 : 0);
  }

  private labeledControl(labelKey: string, control: HTMLElement): HTMLElement {
    const label = document.createElement("label");
    label.className = "agent-control";
    const text = document.createElement("span");
    text.textContent = t(labelKey);
    label.append(text, control);
    return label;
  }

  /**
   * The observation loop, made visible.
   *
   * A continued session receives the previous run's screenshots as native image
   * inputs, so the artifacts of the last run are exactly what the next turn will
   * see. Showing them turns "ask, look, then act" from tribal knowledge into
   * something the panel demonstrates.
   */
  private renderEvidence(session: AgentSession | undefined): HTMLElement | null {
    const runId = session?.run_id;
    if (!runId) return null;
    const artifacts = this.evidence.get(runId) ?? [];
    const images = artifacts.filter((artifact) => artifact.content_type.startsWith("image/"));
    const chip = document.createElement("div");
    chip.className = "agent-evidence";
    chip.dataset.agentFocus = "evidence";
    const label = document.createElement("span");
    label.className = "agent-evidence__label";
    if (images.length === 0) {
      chip.classList.add("agent-evidence--empty");
      label.textContent = t("agent.evidenceNone");
      chip.appendChild(label);
      return chip;
    }
    label.textContent = t("agent.evidenceChip", { images: images.length });
    chip.appendChild(label);
    for (const image of images.slice(0, 4)) {
      const thumb = document.createElement("img");
      thumb.className = "agent-evidence__thumb";
      thumb.src = this.handlers.artifactUrl(runId, image.id);
      thumb.alt = image.name;
      thumb.title = image.name;
      thumb.loading = "lazy";
      chip.appendChild(thumb);
    }
    return chip;
  }

  private renderComposer(draft: string): HTMLElement {
    const form = document.createElement("form");
    form.className = "agent-composer";
    const input = document.createElement("textarea");
    input.className = "input input--code";
    input.rows = 3;
    input.dataset.agentFocus = "composer";
    input.placeholder = t("agent.promptPlaceholder");
    input.value = draft;
    input.addEventListener("input", () => { this.draft = input.value; });
    input.addEventListener("keydown", (event) => {
      if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
        event.preventDefault();
        form.requestSubmit();
      }
    });
    const row = document.createElement("div");
    row.className = "agent-composer__actions";
    const status = document.createElement("span");
    status.className = "muted";
    status.textContent = this.busy ? t("agent.running") : t("agent.ready");
    const hint = document.createElement("span");
    hint.className = "agent-composer__hint";
    hint.textContent = t("agent.ctrlEnterHint");
    const stop = document.createElement("button");
    stop.type = "button";
    stop.className = "btn";
    stop.textContent = t("agent.stop");
    stop.disabled = !this.busy || !this.activeTurnId;
    stop.addEventListener("click", () => void this.stopGeneration());
    const submit = document.createElement("button");
    submit.type = "submit";
    submit.className = "btn btn--primary";
    submit.dataset.agentFocus = "send";
    submit.disabled = !this.desktopAvailable || this.busy || draft.trim() === "";
    submit.textContent = this.busy ? t("agent.running") : t("agent.send");
    row.append(status, hint, stop, submit);
    form.append(input, row);
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      void this.submit(input.value);
    });
    return form;
  }

  private renderLiveTurn(): HTMLElement {
    const card = document.createElement("section");
    card.className = "agent-live";
    const header = document.createElement("div");
    header.className = "agent-live__header";
    const status = document.createElement("strong");
    status.textContent = this.streamPhase || (this.busy ? t("agent.running") : t("agent.ready"));
    const stop = document.createElement("button");
    stop.type = "button";
    stop.className = "btn btn--small";
    stop.textContent = t("agent.stop");
    stop.disabled = !this.busy || !this.activeTurnId;
    stop.addEventListener("click", () => void this.stopGeneration());
    header.append(status, stop);
    card.appendChild(header);
    const draft = document.createElement("details");
    draft.open = Boolean(this.streamText);
    const summary = document.createElement("summary");
    summary.textContent = t("agent.liveDraft");
    const pre = document.createElement("pre");
    pre.className = "agent-stream";
    pre.textContent = this.streamText || t("agent.waitingForDraft");
    draft.append(summary, pre);
    card.appendChild(draft);
    if (this.streamTrace.length > 0) card.appendChild(this.renderTraceList(this.streamTrace));
    if (this.streamWorkflow) card.appendChild(this.renderDiff(this.streamWorkflow));
    return card;
  }

  private renderTraceList(trace: TraceLine[]): HTMLElement {
    const timeline = document.createElement("div");
    timeline.className = "agent-trace";
    for (const entry of trace) {
      const line = document.createElement("div");
      line.className = `agent-trace__line agent-trace__line--${entry.type}`;
      const type = document.createElement("code");
      type.textContent = entry.type;
      const text = document.createElement("span");
      text.textContent = entry.detail ? `${entry.label} · ${entry.detail}` : entry.label;
      line.append(type, text);
      timeline.appendChild(line);
    }
    return timeline;
  }

  private renderDiff(workflow: Workflow): HTMLElement {
    const base = this.settings.baseMode === "last_plan"
      ? this.selectedSession()?.plan?.workflow
      : this.handlers.getCurrentWorkflow();
    return this.renderDiffCard(diffWorkflows(base, workflow));
  }

  private renderDiffCard(diff: WorkflowDiff): HTMLElement {
    const card = document.createElement("section");
    card.className = "agent-diff";
    const title = document.createElement("strong");
    title.textContent = t("agent.planDiff");
    card.appendChild(title);
    if (isWorkflowDiffEmpty(diff)) {
      const empty = document.createElement("p");
      empty.className = "muted";
      empty.textContent = t("agent.noDiff");
      card.appendChild(empty);
      return card;
    }
    const groups: Array<[keyof WorkflowDiff, string]> = [
      ["nodesAdded", "agent.diffNodesAdded"],
      ["nodesChanged", "agent.diffNodesChanged"],
      ["nodesRemoved", "agent.diffNodesRemoved"],
      ["edgesAdded", "agent.diffEdgesAdded"],
      ["edgesChanged", "agent.diffEdgesChanged"],
      ["edgesRemoved", "agent.diffEdgesRemoved"],
    ];
    for (const [key, labelKey] of groups) {
      if (diff[key].length === 0) continue;
      const group = document.createElement("div");
      group.className = "agent-diff__group";
      const label = document.createElement("span");
      label.textContent = `${t(labelKey)} (${diff[key].length})`;
      const values = document.createElement("code");
      values.textContent = diff[key].map((entry) => entry.label).join(", ");
      group.append(label, values);
      card.appendChild(group);
    }
    return card;
  }

  private async submit(goal: string): Promise<void> {
    const trimmed = goal.trim();
    if (!trimmed || this.busy) return;
    this.busy = true;
    this.localError = "";
    this.draft = "";
    this.streamText = "";
    this.streamTrace = [];
    this.streamWorkflow = null;
    this.activeTurnId = globalThis.crypto?.randomUUID?.() ?? `turn-${Date.now().toString(36)}`;
    const templateId = this.templateId;
    // Built-in templates are resolved here, so "observe first" reaches the model
    // exactly like a template the operator wrote.
    const selectedTemplate = findTemplate(templateId, this.settings.templates);
    this.render();
    try {
      await this.handlers.onSubmit(
        {
          goal: trimmed,
          mode: this.settings.mode,
          baseMode: this.settings.baseMode,
          provider: this.providerConfig(),
          sessionId: this.selected ?? undefined,
          extraInstructions: this.settings.extraInstructions,
          promptTemplateId: selectedTemplate?.id,
          promptTemplateInstructions: selectedTemplate?.instructions,
        },
        this.activeTurnId,
        (event) => this.handleStreamEvent(event),
      );
    } catch (error) {
      this.draft = trimmed;
      this.localError = error instanceof Error ? error.message : String(error);
    } finally {
      this.busy = false;
      this.activeTurnId = null;
      this.render();
    }
  }

  private async stopGeneration(): Promise<void> {
    if (!this.activeTurnId) return;
    await this.handlers.onStopGeneration(this.activeTurnId);
    this.busy = false;
    this.activeTurnId = null;
    this.streamPhase = t("agent.cancelled");
    this.render();
  }

  private handleStreamEvent(event: AgentStreamEvent): void {
    switch (event.type) {
      case "turn_started": this.streamPhase = t("agent.phasePlanning"); break;
      case "model_delta":
        this.streamText += event.text;
        if (this.streamFrame === null && typeof requestAnimationFrame === "function") {
          this.streamFrame = requestAnimationFrame(() => {
            this.streamFrame = null;
            this.render();
          });
          return;
        }
        break;
      case "phase_changed": this.streamPhase = event.message || event.phase; break;
      case "validation_result":
        this.streamTrace.push({
          type: "validation",
          label: event.accepted ? t("agent.validationPassed") : t("agent.validationFailed"),
          detail: `${event.errors} error(s), ${event.warnings} warning(s)`,
        });
        break;
      case "repair_started":
        this.streamTrace.push({ type: "repair", label: t("agent.repairRound", { attempt: event.attempt }) });
        break;
      case "plan_ready":
        this.streamWorkflow = event.workflow;
        this.streamTrace.push({ type: "plan", label: event.workflow.id });
        break;
      case "run_finished": this.streamTrace.push({ type: "run", label: t("agent.runFinished") }); break;
      case "completed":
        if (event.response.trace) this.streamTrace.push({ type: "trace", label: `${event.response.trace.length} trace entries` });
        break;
      case "failed":
        this.localError = event.message;
        this.streamTrace.push({ type: "error", label: event.message });
        break;
      case "cancelled":
        this.streamPhase = t("agent.cancelled");
        this.streamTrace.push({ type: "cancel", label: t("agent.cancelled") });
        break;
    }
    this.render();
  }

  private renderDetail(session: AgentSession): HTMLElement {
    const detail = document.createElement("div");
    detail.className = "session-detail";
    const meta = document.createElement("p");
    meta.className = "muted";
    meta.textContent = t("agent.tokens", { provider: session.provider || "agent", tokens: session.tokens_used });
    detail.appendChild(meta);
    for (const approval of session.approvals) detail.appendChild(this.renderApproval(session, approval));
    if (session.plan) detail.appendChild(this.renderPlan(session));
    const trace = this.sessionTraces.get(session.id);
    if (trace && trace.length > 0) detail.appendChild(this.renderStoredTrace(session.id, trace));
    if (session.run_id) {
      const actions = document.createElement("div");
      actions.className = "session-run-actions";
      const run = document.createElement("button");
      run.type = "button";
      run.className = "btn btn--small";
      run.dataset.agentFocus = `open-run.${session.id}`;
      run.textContent = t("actions.openRun", { id: session.run_id.slice(0, 8) });
      run.addEventListener("click", () => this.handlers.onOpenRun(session.run_id!));
      const audit = document.createElement("button");
      audit.type = "button";
      audit.className = "btn btn--small";
      audit.dataset.agentFocus = `open-audit.${session.id}`;
      audit.textContent = t("actions.openAudit", { id: session.run_id.slice(0, 8) });
      audit.addEventListener("click", () => this.handlers.onOpenAudit(session.run_id!));
      actions.append(run, audit);
      if (session.status === "awaiting_approval" || session.status === "running") {
        const resume = document.createElement("button");
        resume.type = "button";
        resume.className = "btn btn--small";
        resume.textContent = t("actions.resume");
        resume.addEventListener("click", () => this.handlers.onResumeRun(session.run_id!));
        actions.appendChild(resume);
      }
      detail.appendChild(actions);
    }
    if (session.messages.length > 0) {
      const heading = document.createElement("h4");
      heading.className = "inspector__section";
      heading.textContent = t("agent.conversation");
      detail.appendChild(heading);
      const conversation = document.createElement("div");
      conversation.className = "conversation";
      for (const message of session.messages) {
        const line = document.createElement("p");
        line.className = `message message--${message.role}`;
        const who = document.createElement("span");
        who.className = "message__who";
        who.textContent = message.role;
        const text = document.createElement("span");
        text.textContent = message.text;
        line.append(who, text);
        if (message.role === "operator") {
          const reuse = document.createElement("button");
          reuse.type = "button";
          reuse.className = "btn btn--small message__reuse";
          reuse.textContent = t("agent.reusePrompt");
          reuse.addEventListener("click", () => {
            this.draft = message.text;
            this.render();
          });
          line.appendChild(reuse);
        }
        conversation.appendChild(line);
      }
      detail.appendChild(conversation);
    }
    return detail;
  }

  private renderApproval(session: AgentSession, approval: ApprovalRequest): HTMLElement {
    const card = document.createElement("div");
    card.className = "approval";
    if (approval.decision) card.classList.add(`approval--${approval.decision}`);
    const title = document.createElement("p");
    title.className = "approval__title";
    title.textContent = approval.decision
      ? t(approval.decision === "approved" ? "agent.approved" : "agent.denied", { by: approval.decided_by ?? "operator" })
      : t("agent.approvalRequired");
    const body = document.createElement("p");
    body.className = "approval__body";
    body.textContent = approval.reason;
    const permissions = document.createElement("p");
    permissions.className = "approval__permissions";
    permissions.textContent = t("agent.permissions", {
      node: approval.node_id,
      type: approval.node_type,
      permissions: approval.permissions.join(", ") || t("agent.noneDeclared"),
    });
    const input = document.createElement("pre");
    input.className = "approval__input";
    input.textContent = JSON.stringify(approval.input, null, 2);
    card.append(title, body, permissions, input);
    if (!approval.decision) {
      const actions = document.createElement("div");
      actions.className = "approval__actions";
      const approve = document.createElement("button");
      approve.type = "button";
      approve.className = "btn btn--primary btn--small";
      approve.textContent = t("actions.approve");
      approve.addEventListener("click", () => this.handlers.onDecide(session.id, approval.id, true));
      const deny = document.createElement("button");
      deny.type = "button";
      deny.className = "btn btn--small";
      deny.textContent = t("actions.deny");
      deny.addEventListener("click", () => this.handlers.onDecide(session.id, approval.id, false));
      actions.append(approve, deny);
      card.appendChild(actions);
    }
    return card;
  }

  private renderPlan(session: AgentSession): HTMLElement {
    const plan = session.plan!;
    const card = document.createElement("section");
    card.className = "plan";
    const title = document.createElement("p");
    title.className = "plan__title";
    title.textContent = plan.valid ? t("agent.plan", { id: plan.workflow.id }) : t("agent.planRejected", { errors: plan.errors });
    const summary = document.createElement("p");
    summary.className = "muted";
    summary.textContent = t("agent.planSummary", {
      nodes: plan.workflow.nodes.length,
      edges: plan.workflow.edges.length,
      warnings: plan.warnings,
    });
    card.append(title, summary, this.renderDiff(plan.workflow));
    for (const diagnostic of plan.diagnostics) {
      const line = document.createElement("p");
      line.className = `problem problem--${diagnostic.severity}`;
      const localized = localizeDiagnostic(diagnostic);
      line.textContent = `[${localized.code}] ${localized.message}`;
      card.appendChild(line);
    }
    const json = document.createElement("details");
    json.className = "agent-plan-json";
    json.dataset.sessionId = session.id;
    json.open = this.planJsonOpen.has(session.id);
    json.addEventListener("toggle", () => {
      if (json.open) this.planJsonOpen.add(session.id);
      else this.planJsonOpen.delete(session.id);
    });
    const jsonSummary = document.createElement("summary");
    jsonSummary.dataset.agentFocus = `plan-json.${session.id}`;
    jsonSummary.textContent = t("agent.finalWorkflowJson");
    const pre = document.createElement("pre");
    pre.textContent = JSON.stringify(plan.workflow, null, 2);
    json.append(jsonSummary, pre);
    card.appendChild(json);
    const actions = document.createElement("div");
    actions.className = "agent-plan-actions";
    const validate = document.createElement("button");
    validate.type = "button";
    validate.className = "btn btn--small";
    validate.dataset.agentFocus = `validate-plan.${session.id}`;
    validate.textContent = t("actions.validate");
    validate.addEventListener("click", () => void this.validatePlan(plan.workflow, card));
    const load = document.createElement("button");
    load.type = "button";
    load.className = "btn btn--small";
    load.dataset.agentFocus = `load-plan.${session.id}`;
    load.textContent = t("actions.loadPlan");
    load.disabled = !plan.valid;
    load.addEventListener("click", () => this.handlers.onLoadPlan(session.id));
    const run = document.createElement("button");
    run.type = "button";
    run.className = "btn btn--primary btn--small";
    run.dataset.agentFocus = `run-plan.${session.id}`;
    run.textContent = t("actions.runPlan");
    run.disabled = !plan.valid || this.settings.mode === "forbidden";
    run.addEventListener("click", () => void this.handlers.onRunPlan(plan.workflow, session.id, this.settings.mode));
    const retry = document.createElement("button");
    retry.type = "button";
    retry.className = "btn btn--small";
    retry.textContent = t("agent.retry");
    retry.addEventListener("click", () => void this.submit(session.goal));
    actions.append(validate, load, run, retry);
    card.appendChild(actions);
    return card;
  }

  private renderStoredTrace(sessionId: string, trace: unknown[]): HTMLElement {
    const details = document.createElement("details");
    details.className = "agent-trace-details";
    details.open = this.traceOpen.has(sessionId);
    details.addEventListener("toggle", () => {
      if (details.open) this.traceOpen.add(sessionId);
      else this.traceOpen.delete(sessionId);
    });
    const summary = document.createElement("summary");
    summary.textContent = `${t("agent.trace")} (${trace.length})`;
    details.appendChild(summary);
    const body = document.createElement("div");
    body.className = "agent-trace";
    for (const raw of trace) {
      const entry = raw as { step?: string; summary?: string; seq?: number };
      const line = document.createElement("div");
      line.className = "agent-trace__line";
      const type = document.createElement("code");
      type.textContent = entry.step ?? "event";
      const text = document.createElement("span");
      text.textContent = `${entry.seq ?? ""} ${entry.summary ?? JSON.stringify(raw)}`.trim();
      line.append(type, text);
      body.appendChild(line);
    }
    details.appendChild(body);
    return details;
  }

  private async validatePlan(workflow: Workflow, card: HTMLElement): Promise<void> {
    const existing = card.querySelector(".agent-validation-result");
    existing?.remove();
    try {
      const report = await this.handlers.onValidatePlan(workflow);
      const line = document.createElement("p");
      line.className = `agent-validation-result problem problem--${report.diagnostics.some((d) => d.severity === "error") ? "error" : "info"}`;
      line.textContent = t("agent.validationResult", {
        errors: report.diagnostics.filter((d) => d.severity === "error").length,
        warnings: report.diagnostics.filter((d) => d.severity === "warning").length,
      });
      card.appendChild(line);
    } catch (error) {
      const line = document.createElement("p");
      line.className = "agent-validation-result problem problem--error";
      line.textContent = error instanceof Error ? error.message : String(error);
      card.appendChild(line);
    }
  }
}
