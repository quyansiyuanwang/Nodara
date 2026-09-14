/**
 * The agent panel.
 *
 * The Studio never calls the agent. It reads sessions from the runtime — the
 * session API the architecture document designates as the channel between the
 * two — and writes back only one thing: an operator's approval decision.
 *
 * That is why approving here is meaningful rather than cosmetic. The runtime's
 * approval handler is blocking the run thread while it waits; this button is
 * what releases it.
 */

import { localizeAgentStatus, localizeDiagnostic, t } from "../i18n";
import { AgentSession, ApprovalRequest, AgentSessionList } from "../runtime/types";

export interface AgentPanelHandlers {
  /** Decide a pending approval. */
  onDecide: (sessionId: string, approvalId: string, approve: boolean) => void;
  /** Load a planned workflow into the editor. */
  onLoadPlan: (sessionId: string) => void;
  /** Open the run this session started. */
  onOpenRun: (runId: string) => void;
}

export class AgentPanel {
  private sessions: AgentSession[] = [];
  private selected: string | null = null;

  constructor(
    private readonly root: HTMLElement,
    private readonly handlers: AgentPanelHandlers,
  ) {}

  /** Replace the session list. Preserves the current selection when possible. */
  setSessions(list: AgentSessionList): void {
    this.sessions = list.sessions;
    const pending = list.pending_approvals;
    if (this.selected && !this.sessions.some((session) => session.id === this.selected)) {
      this.selected = null;
    }
    if (!this.selected) {
      // Prefer a session that needs the operator's attention.
      this.selected =
        pending[0]?.session_id ?? this.sessions[0]?.id ?? null;
    }
    this.render();
  }

  /** Session currently shown, if any. */
  selectedSession(): AgentSession | undefined {
    return this.sessions.find((session) => session.id === this.selected);
  }

  /** True when at least one approval is blocking a run. */
  static needsAttention(list: AgentSessionList): boolean {
    return list.pending_approvals.length > 0;
  }

  private render(): void {
    this.root.replaceChildren();
    if (this.sessions.length === 0) {
      const empty = document.createElement("p");
      empty.className = "muted";
      empty.textContent = t("agent.empty");
      this.root.appendChild(empty);
      return;
    }

    const list = document.createElement("div");
    list.className = "sessions";
    for (const session of this.sessions) {
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
    this.root.appendChild(list);

    const session = this.selectedSession();
    if (session) {
      this.root.appendChild(this.renderDetail(session));
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

    if (session.plan) {
      detail.appendChild(this.renderPlan(session));
    }

    if (session.run_id) {
      const runButton = document.createElement("button");
      runButton.type = "button";
      runButton.className = "btn btn--small";
      runButton.textContent = t("actions.openRun", { id: session.run_id.slice(0, 8) });
      runButton.addEventListener("click", () => this.handlers.onOpenRun(session.run_id!));
      detail.appendChild(runButton);
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
      approve.addEventListener("click", () =>
        this.handlers.onDecide(session.id, approval.id, true),
      );

      const deny = document.createElement("button");
      deny.type = "button";
      deny.className = "btn btn--small";
      deny.textContent = t("actions.deny");
      deny.addEventListener("click", () =>
        this.handlers.onDecide(session.id, approval.id, false),
      );

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

    const nodes = document.createElement("ul");
    nodes.className = "plan__nodes";
    for (const node of plan.workflow.nodes) {
      const item = document.createElement("li");
      item.textContent = `${node.label ?? node.id} — ${node.type}`;
      nodes.appendChild(item);
    }
    card.appendChild(nodes);

    for (const diagnostic of plan.diagnostics) {
      const line = document.createElement("p");
      line.className = `problem problem--${diagnostic.severity}`;
      const localized = localizeDiagnostic(diagnostic);
      line.textContent = `[${localized.code}] ${localized.message}`;
      card.appendChild(line);
    }

    const load = document.createElement("button");
    load.type = "button";
    load.className = "btn btn--small";
    load.textContent = t("actions.loadPlan");
    load.addEventListener("click", () => this.handlers.onLoadPlan(session.id));
    card.appendChild(load);

    return card;
  }
}
