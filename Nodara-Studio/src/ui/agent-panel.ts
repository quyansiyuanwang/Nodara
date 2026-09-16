/**
 * The conversational Agent panel.
 *
 * Runtime sessions are the source of truth. The desktop shell starts a
 * `nodara-agent studio` process for each turn, while session polling keeps the
 * transcript, plan, approvals, run and audit link live.
 */

import { localizeAgentStatus, localizeDiagnostic, t } from "../i18n";
import {
  AgentSession,
  AgentSessionList,
  ApprovalRequest,
  ValidationReport,
  Workflow,
} from "../runtime/types";

export type AgentExecutionMode = "forbidden" | "manual" | "partial" | "all";
export type AgentBaseMode = "current" | "last_plan";

export interface AgentProviderConfig {
  endpoint: string;
  model: string;
  apiKey: string;
  timeoutMs: number;
}

export interface AgentSubmitRequest {
  goal: string;
  mode: AgentExecutionMode;
  baseMode: AgentBaseMode;
  provider: AgentProviderConfig;
  sessionId?: string;
}

interface AgentInputFocus {
  field: string;
  start: number | null;
  end: number | null;
}

export interface AgentPanelHandlers {
  onSubmit: (request: AgentSubmitRequest) => Promise<void>;
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
  private mode: AgentExecutionMode = "partial";
  private baseMode: AgentBaseMode = "current";
  private provider: AgentProviderConfig = {
    endpoint: "https://api.openai.com/v1/chat/completions",
    model: "gpt-4o-mini",
    apiKey: "",
    timeoutMs: 300_000,
  };
  private providerOpen = false;
  private readonly planJsonOpen = new Set<string>();
  private sessionFingerprint = "";
  private sessionFilter = "";

  constructor(
    private readonly root: HTMLElement,
    private readonly handlers: AgentPanelHandlers,
    private readonly desktopAvailable = true,
  ) {
    this.render();
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

  /** True when at least one approval is blocking a run. */
  static needsAttention(list: AgentSessionList): boolean {
    return list.pending_approvals.length > 0;
  }

  private captureInputFocus(): AgentInputFocus | null {
    const active = document.activeElement;
    if (
      !(active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement) ||
      !this.root.contains(active)
    ) {
      return null;
    }
    const field = active.dataset.agentFocus;
    if (!field) return null;
    return {
      field,
      start: active.selectionStart,
      end: active.selectionEnd,
    };
  }

  private restoreInputFocus(focus: AgentInputFocus | null): void {
    if (!focus) return;
    const target = this.root.querySelector<HTMLInputElement | HTMLTextAreaElement>(
      `[data-agent-focus="${focus.field}"]`,
    );
    if (!target) return;
    target.focus();
    if (focus.start === null || focus.end === null) return;
    try {
      target.setSelectionRange(focus.start, focus.end);
    } catch {
      // Number inputs do not expose a text selection range.
    }
  }

  private render(): void {
    const focus = this.captureInputFocus();
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
    if (!this.desktopAvailable) {
      const desktop = document.createElement("p");
      desktop.className = "gate agent-desktop-only";
      desktop.textContent = t("agent.desktopOnly");
      main.appendChild(desktop);
    }
    main.appendChild(this.renderProviderSettings());
    main.appendChild(this.renderControls());
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
    this.restoreInputFocus(focus);
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
    details.addEventListener("toggle", () => {
      this.providerOpen = details.open;
    });
    const summary = document.createElement("summary");
    const summaryTitle = document.createElement("span");
    summaryTitle.textContent = t("agent.providerSettings");
    const summaryModel = document.createElement("code");
    summaryModel.textContent = this.provider.model;
    summary.append(summaryTitle, summaryModel);
    details.appendChild(summary);
    const grid = document.createElement("div");
    grid.className = "agent-provider__grid";
    const fields: Array<[keyof AgentProviderConfig, string, string, string]> = [
      ["endpoint", "agent.endpoint", "text", this.provider.endpoint],
      ["model", "agent.model", "text", this.provider.model],
      ["apiKey", "agent.apiKey", "password", this.provider.apiKey],
      ["timeoutMs", "agent.timeout", "number", String(this.provider.timeoutMs)],
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
      input.autocomplete = key === "apiKey" ? "off" : "on";
      input.addEventListener("input", () => {
        if (key === "timeoutMs") this.provider.timeoutMs = Number(input.value) || 300_000;
        else this.provider[key] = input.value;
      });
      label.append(title, input);
      grid.appendChild(label);
    }
    const hint = document.createElement("p");
    hint.className = "field__hint";
    hint.textContent = t("agent.apiKeyHint");
    grid.appendChild(hint);
    details.appendChild(grid);
    return details;
  }

  private renderControls(): HTMLElement {
    const controls = document.createElement("div");
    controls.className = "agent-controls";
    const mode = document.createElement("select");
    mode.className = "input input--small";
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
    mode.value = this.mode;
    mode.addEventListener("change", () => {
      this.mode = mode.value as AgentExecutionMode;
      this.render();
    });
    const modeLabel = document.createElement("label");
    modeLabel.className = "agent-control";
    const modeText = document.createElement("span");
    modeText.textContent = t("agent.modeLabel");
    modeLabel.append(modeText, mode);
    const base = document.createElement("select");
    base.className = "input input--small";
    for (const [value, key] of [
      ["current", "agent.baseCurrent"],
      ["last_plan", "agent.baseLastPlan"],
    ] as const) {
      const option = document.createElement("option");
      option.value = value;
      option.textContent = t(key);
      base.appendChild(option);
    }
    base.value = this.baseMode;
    base.disabled = !this.selectedSession()?.plan;
    base.addEventListener("change", () => {
      this.baseMode = base.value as AgentBaseMode;
    });
    const baseLabel = document.createElement("label");
    baseLabel.className = "agent-control";
    const baseText = document.createElement("span");
    baseText.textContent = t("agent.baseLabel");
    baseLabel.append(baseText, base);
    controls.append(modeLabel, baseLabel);
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
    input.addEventListener("input", () => {
      this.draft = input.value;
    });
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
    const shortcut = document.createElement("span");
    shortcut.className = "agent-composer__hint";
    shortcut.textContent = t("agent.ctrlEnterHint");
    const submit = document.createElement("button");
    submit.type = "submit";
    submit.className = "btn btn--primary";
    submit.disabled = !this.desktopAvailable || this.busy || draft.trim() === "";
    submit.textContent = this.busy ? t("agent.running") : t("agent.send");
    row.append(status, shortcut, submit);
    form.append(input, row);
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      void this.submit(input.value);
    });
    return form;
  }

  private async submit(goal: string): Promise<void> {
    const trimmed = goal.trim();
    if (!trimmed || this.busy) return;
    this.busy = true;
    this.localError = "";
    this.draft = "";
    this.render();
    try {
      await this.handlers.onSubmit({
        goal: trimmed,
        mode: this.mode,
        baseMode: this.baseMode,
        provider: { ...this.provider },
        sessionId: this.selected ?? undefined,
      });
    } catch (error) {
      this.draft = trimmed;
      this.localError = error instanceof Error ? error.message : String(error);
    } finally {
      this.busy = false;
      this.render();
    }
  }

  private renderDetail(session: AgentSession): HTMLElement {
    const detail = document.createElement("div");
    detail.className = "session-detail";
    const meta = document.createElement("p");
    meta.className = "muted";
    meta.textContent = t("agent.tokens", { provider: session.provider || "agent", tokens: session.tokens_used });
    detail.appendChild(meta);

    for (const approval of session.approvals) {
      detail.appendChild(this.renderApproval(session, approval));
    }
    if (session.plan) detail.appendChild(this.renderPlan(session));
    if (session.run_id) {
      const runActions = document.createElement("div");
      runActions.className = "session-run-actions";
      const runButton = document.createElement("button");
      runButton.type = "button";
      runButton.className = "btn btn--small";
      runButton.textContent = t("actions.openRun", { id: session.run_id.slice(0, 8) });
      runButton.addEventListener("click", () => this.handlers.onOpenRun(session.run_id!));
      runActions.appendChild(runButton);
      const auditButton = document.createElement("button");
      auditButton.type = "button";
      auditButton.className = "btn btn--small";
      auditButton.textContent = t("actions.openAudit", { id: session.run_id.slice(0, 8) });
      auditButton.addEventListener("click", () => this.handlers.onOpenAudit(session.run_id!));
      runActions.appendChild(auditButton);
      if (session.status === "awaiting_approval" || session.status === "running") {
        const resume = document.createElement("button");
        resume.type = "button";
        resume.className = "btn btn--small";
        resume.textContent = t("actions.resume");
        resume.addEventListener("click", () => this.handlers.onResumeRun(session.run_id!));
        runActions.appendChild(resume);
      }
      detail.appendChild(runActions);
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
      ? t(approval.decision === "approved" ? "agent.approved" : "agent.denied", {
          by: approval.decided_by ?? "operator",
        })
      : t("agent.approvalRequired");
    card.appendChild(title);
    const body = document.createElement("p");
    body.className = "approval__body";
    body.textContent = approval.reason;
    card.appendChild(body);
    const permissions = document.createElement("p");
    permissions.className = "approval__permissions";
    permissions.textContent = t("agent.permissions", {
      node: approval.node_id,
      type: approval.node_type,
      permissions: approval.permissions.join(", ") || t("agent.noneDeclared"),
    });
    card.appendChild(permissions);
    const input = document.createElement("pre");
    input.className = "approval__input";
    input.textContent = JSON.stringify(approval.input, null, 2);
    card.appendChild(input);
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
    const card = document.createElement("div");
    card.className = "plan";
    const title = document.createElement("p");
    title.className = "plan__title";
    title.textContent = plan.valid
      ? t("agent.plan", { id: plan.workflow.id })
      : t("agent.planRejected", { errors: plan.errors });
    card.appendChild(title);
    const summary = document.createElement("p");
    summary.className = "muted";
    summary.textContent = t("agent.planSummary", {
      nodes: plan.workflow.nodes.length,
      edges: plan.workflow.edges.length,
      warnings: plan.warnings,
    });
    card.appendChild(summary);
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
    validate.textContent = t("actions.validate");
    validate.addEventListener("click", () => void this.validatePlan(plan.workflow, card));
    const load = document.createElement("button");
    load.type = "button";
    load.className = "btn btn--small";
    load.textContent = t("actions.loadPlan");
    load.disabled = !plan.valid;
    load.addEventListener("click", () => this.handlers.onLoadPlan(session.id));
    const run = document.createElement("button");
    run.type = "button";
    run.className = "btn btn--primary btn--small";
    run.textContent = t("actions.runPlan");
    run.disabled = !plan.valid || this.mode === "forbidden";
    run.addEventListener("click", () => void this.handlers.onRunPlan(plan.workflow, session.id, this.mode));
    actions.append(validate, load, run);
    card.appendChild(actions);
    return card;
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
