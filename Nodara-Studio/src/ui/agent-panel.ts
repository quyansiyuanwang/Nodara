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
  defaultAgentSettings,
  loadAgentSettings,
  newProfile,
  newTemplate,
  saveAgentSettings,
} from "../model/agent-settings";
import { diffWorkflows, isWorkflowDiffEmpty, WorkflowDiff } from "../model/workflow-diff";
import {
  AgentSession,
  AgentSessionList,
  ApprovalRequest,
  ValidationReport,
  Workflow,
} from "../runtime/types";

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
}

export class AgentPanel {
  private sessions: AgentSession[] = [];
  private selected: string | null = null;
  private draft = "";
  private busy = false;
  private localError = "";
  private settings: AgentSettings = loadAgentSettings();
  private apiKey = "";
  private providerOpen = false;
  private readonly planJsonOpen = new Set<string>();
  private readonly traceOpen = new Set<string>();
  private readonly sessionTraces = new Map<string, unknown[]>();
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
    this.render();
    void this.loadCredential();
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
    return { ...profile, profileId: profile.id, apiKey: this.apiKey };
  }

  private persist(): void {
    this.settings.workspaceOpen = this.workspaceOpen;
    saveAgentSettings(this.settings);
  }

  private async loadCredential(): Promise<void> {
    if (!this.desktopAvailable) return;
    try {
      this.apiKey = (await this.handlers.onCredentialGet(this.activeProfile().id)) ?? "";
      this.render();
    } catch (error) {
      this.localError = error instanceof Error ? error.message : String(error);
      this.render();
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
    const provider = this.root.querySelector<HTMLDetailsElement>(".agent-provider");
    if (provider) this.providerOpen = provider.open;
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
    const workspaceHeader = document.createElement("div");
    workspaceHeader.className = "agent-workspace-header";
    const workspaceTitle = document.createElement("strong");
    workspaceTitle.textContent = t("agent.workspace");
    const workspaceToggle = document.createElement("button");
    workspaceToggle.type = "button";
    workspaceToggle.className = "btn btn--small";
    workspaceToggle.dataset.agentFocus = "workspace-toggle";
    workspaceToggle.textContent = this.workspaceOpen
      ? t("agent.collapseWorkspace")
      : t("agent.expandWorkspace");
    workspaceToggle.addEventListener("click", () => {
      this.workspaceOpen = !this.workspaceOpen;
      this.persist();
      this.render();
    });
    workspaceHeader.append(workspaceTitle, workspaceToggle);
    main.appendChild(workspaceHeader);
    if (!this.desktopAvailable) {
      const desktop = document.createElement("p");
      desktop.className = "gate agent-desktop-only";
      desktop.textContent = t("agent.desktopOnly");
      main.appendChild(desktop);
    }
    main.appendChild(this.renderProviderSettings());
    main.appendChild(this.renderControls());
    if (this.busy || this.streamText || this.streamTrace.length > 0) {
      main.appendChild(this.renderLiveTurn());
    }
    const session = this.selectedSession();
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

  private renderProviderSettings(): HTMLElement {
    const details = document.createElement("details");
    details.className = "agent-provider";
    details.open = this.providerOpen;
    details.addEventListener("toggle", () => { this.providerOpen = details.open; });
    const summary = document.createElement("summary");
    summary.dataset.agentFocus = "provider-summary";
    const summaryTitle = document.createElement("span");
    summaryTitle.textContent = t("agent.providerSettings");
    const summaryModel = document.createElement("code");
    summaryModel.textContent = `${this.activeProfile().name} · ${this.activeProfile().model}`;
    summary.append(summaryTitle, summaryModel);
    details.appendChild(summary);

    const grid = document.createElement("div");
    grid.className = "agent-provider__grid";
    const profile = this.activeProfile();
    const profileRow = document.createElement("div");
    profileRow.className = "agent-profile-row";
    const profileSelect = document.createElement("select");
    profileSelect.className = "input input--small";
    profileSelect.dataset.agentFocus = "provider-profile";
    for (const item of this.settings.profiles) {
      const option = document.createElement("option");
      option.value = item.id;
      option.textContent = item.name;
      profileSelect.appendChild(option);
    }
    profileSelect.value = profile.id;
    profileSelect.addEventListener("change", () => {
      this.settings.activeProfileId = profileSelect.value;
      this.apiKey = "";
      this.persist();
      void this.loadCredential();
    });
    const addProfile = document.createElement("button");
    addProfile.type = "button";
    addProfile.className = "btn btn--small";
    addProfile.textContent = t("agent.addProfile");
    addProfile.addEventListener("click", () => {
      const next = newProfile(this.settings.profiles.length + 1);
      this.settings.profiles.push(next);
      this.settings.activeProfileId = next.id;
      this.apiKey = "";
      this.persist();
      this.render();
    });
    const removeProfile = document.createElement("button");
    removeProfile.type = "button";
    removeProfile.className = "btn btn--small";
    removeProfile.textContent = t("agent.removeProfile");
    removeProfile.disabled = this.settings.profiles.length <= 1;
    removeProfile.addEventListener("click", () => {
      const removed = profile.id;
      this.settings.profiles = this.settings.profiles.filter((item) => item.id !== removed);
      this.settings.activeProfileId = this.settings.profiles[0].id;
      this.apiKey = "";
      this.persist();
      void this.handlers.onCredentialDelete(removed).catch(() => undefined);
      this.render();
    });
    profileRow.append(profileSelect, addProfile, removeProfile);
    grid.appendChild(profileRow);

    const fields: Array<[keyof AgentProviderProfile, string, string, string]> = [
      ["name", "agent.profileName", "text", profile.name],
      ["endpoint", "agent.endpoint", "text", profile.endpoint],
      ["model", "agent.model", "text", profile.model],
      ["timeoutMs", "agent.timeout", "number", String(profile.timeoutMs)],
    ];
    for (const [key, labelKey, type, value] of fields) {
      const label = document.createElement("label");
      label.className = "field";
      const title = document.createElement("span");
      title.className = "field__label";
      title.textContent = t(labelKey);
      const input = document.createElement("input");
      input.className = "input";
      input.type = type;
      input.value = value;
      input.dataset.agentFocus = `provider.${key}`;
      input.addEventListener("input", () => {
        const target = this.settings.profiles.find((item) => item.id === this.settings.activeProfileId);
        if (!target) return;
        if (key === "timeoutMs") target.timeoutMs = Number(input.value) || 300_000;
        else if (key === "name" || key === "endpoint" || key === "model") target[key] = input.value;
        this.persist();
      });
      label.append(title, input);
      grid.appendChild(label);
    }

    const keyLabel = document.createElement("label");
    keyLabel.className = "field";
    const keyTitle = document.createElement("span");
    keyTitle.className = "field__label";
    keyTitle.textContent = t("agent.apiKey");
    const keyInput = document.createElement("input");
    keyInput.className = "input";
    keyInput.type = "password";
    keyInput.value = this.apiKey;
    keyInput.autocomplete = "off";
    keyInput.dataset.agentFocus = "provider.apiKey";
    keyInput.addEventListener("change", () => {
      this.apiKey = keyInput.value;
      void this.handlers.onCredentialSet(profile.id, keyInput.value).catch((error) => {
        this.localError = error instanceof Error ? error.message : String(error);
        this.render();
      });
    });
    keyLabel.append(keyTitle, keyInput);
    grid.appendChild(keyLabel);

    const hint = document.createElement("p");
    hint.className = "field__hint";
    hint.textContent = t("agent.apiKeyHint");
    const visionHint = document.createElement("p");
    visionHint.className = "field__hint";
    visionHint.textContent = t("agent.visionRuntimeHint");
    grid.append(hint, visionHint, this.renderPromptTemplates());
    details.appendChild(grid);
    return details;
  }

  private renderPromptTemplates(): HTMLElement {
    const wrapper = document.createElement("div");
    wrapper.className = "agent-templates";
    const heading = document.createElement("div");
    heading.className = "agent-templates__heading";
    const title = document.createElement("strong");
    title.textContent = t("agent.promptTemplates");
    const add = document.createElement("button");
    add.type = "button";
    add.className = "btn btn--small";
    add.textContent = t("agent.addTemplate");
    add.addEventListener("click", () => {
      this.settings.templates.push(newTemplate(this.settings.templates.length + 1));
      this.persist();
      this.render();
    });
    heading.append(title, add);
    wrapper.appendChild(heading);
    if (this.settings.templates.length === 0) {
      const empty = document.createElement("p");
      empty.className = "muted";
      empty.textContent = t("agent.noTemplates");
      wrapper.appendChild(empty);
      return wrapper;
    }
    for (const template of this.settings.templates) {
      const row = document.createElement("div");
      row.className = "agent-template";
      const name = document.createElement("input");
      name.className = "input input--small";
      name.value = template.name;
      name.dataset.agentFocus = `template.name.${template.id}`;
      name.addEventListener("change", () => { template.name = name.value; this.persist(); });
      const instructions = document.createElement("textarea");
      instructions.className = "input input--code";
      instructions.rows = 2;
      instructions.placeholder = t("agent.templateInstructions");
      instructions.value = template.instructions;
      instructions.dataset.agentFocus = `template.instructions.${template.id}`;
      instructions.addEventListener("change", () => {
        template.instructions = instructions.value;
        this.persist();
      });
      const remove = document.createElement("button");
      remove.type = "button";
      remove.className = "btn btn--small";
      remove.textContent = t("actions.delete");
      remove.addEventListener("click", () => {
        this.settings.templates = this.settings.templates.filter((item) => item.id !== template.id);
        this.persist();
        this.render();
      });
      row.append(name, instructions, remove);
      wrapper.appendChild(row);
    }
    return wrapper;
  }

  private renderControls(): HTMLElement {
    const controls = document.createElement("div");
    controls.className = "agent-controls";
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
    });
    const modeLabel = document.createElement("label");
    modeLabel.className = "agent-control";
    const modeText = document.createElement("span");
    modeText.textContent = t("agent.modeLabel");
    modeLabel.append(modeText, mode);
    const base = document.createElement("select");
    base.className = "input input--small";
    base.dataset.agentFocus = "baseline-mode";
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
    base.disabled = !this.selectedSession()?.plan;
    base.addEventListener("change", () => {
      this.settings.baseMode = base.value as AgentBaseMode;
      this.persist();
    });
    const baseLabel = document.createElement("label");
    baseLabel.className = "agent-control";
    const baseText = document.createElement("span");
    baseText.textContent = t("agent.baseLabel");
    baseLabel.append(baseText, base);
    controls.append(modeLabel, baseLabel);
    const extra = document.createElement("textarea");
    extra.className = "input input--code agent-extra-instructions";
    extra.rows = 2;
    extra.placeholder = t("agent.extraInstructions");
    extra.value = this.settings.extraInstructions;
    extra.dataset.agentFocus = "extra-instructions";
    extra.addEventListener("change", () => {
      this.settings.extraInstructions = extra.value;
      this.persist();
    });
    controls.appendChild(extra);
    return controls;
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
    const template = document.createElement("select");
    template.className = "input input--small";
    template.dataset.agentFocus = "prompt-template";
    const none = document.createElement("option");
    none.value = "";
    none.textContent = t("agent.noTemplate");
    template.appendChild(none);
    for (const item of this.settings.templates) {
      const option = document.createElement("option");
      option.value = item.id;
      option.textContent = item.name;
      template.appendChild(option);
    }
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
    row.append(status, template, stop, submit);
    form.append(input, row);
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      void this.submit(input.value, template.value);
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

  private async submit(goal: string, templateId: string): Promise<void> {
    const trimmed = goal.trim();
    if (!trimmed || this.busy) return;
    this.busy = true;
    this.localError = "";
    this.draft = "";
    this.streamText = "";
    this.streamTrace = [];
    this.streamWorkflow = null;
    this.activeTurnId = globalThis.crypto?.randomUUID?.() ?? `turn-${Date.now().toString(36)}`;
    const selectedTemplate = this.settings.templates.find((item) => item.id === templateId);
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
          promptTemplateId: templateId || undefined,
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
    retry.addEventListener("click", () => void this.submit(session.goal, ""));
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
