/** Graphical audit workspace backed by durable records and run events. */

import { localizeAuditCategory, localizeRunStatus, t } from "../i18n";
import {
  AuditRecord,
  EventEnvelope,
  RunSnapshot,
  Workflow,
} from "../runtime/types";

export interface AuditContext {
  run: RunSnapshot;
  workflow: Workflow;
  events: EventEnvelope[];
}

export class AuditPanel {
  private records: AuditRecord[] = [];
  private context: AuditContext | null = null;
  private selectedRecord: AuditRecord | null = null;
  private selectedNode: string | null = null;
  private workspaceOpen = false;

  constructor(private readonly root: HTMLElement) {}

  setRecords(records: AuditRecord[]): void {
    this.records = records;
    if (this.selectedRecord && !records.some((record) => record.seq === this.selectedRecord!.seq)) {
      this.selectedRecord = null;
    }
    this.render();
  }

  setContext(context: AuditContext | null): void {
    this.context = context;
    if (this.selectedNode && !context?.workflow.nodes.some((node) => node.id === this.selectedNode)) {
      this.selectedNode = null;
    }
    this.render();
  }

  private render(): void {
    this.root.replaceChildren();
    this.root.classList.toggle("audit--workspace", this.workspaceOpen);
    document.body.classList.toggle("audit-workspace-open", this.workspaceOpen);
    if (!this.context) {
      this.renderCatalogue();
      return;
    }
    const shell = document.createElement("div");
    shell.className = "audit-workspace";
    shell.appendChild(this.renderAuditHeader());
    shell.appendChild(this.renderSummary());
    const grid = document.createElement("div");
    grid.className = "audit-workspace__grid";
    grid.appendChild(this.renderGraph());
    grid.appendChild(this.renderTimeline());
    grid.appendChild(this.renderDetail());
    shell.appendChild(grid);
    this.root.appendChild(shell);
  }

  private renderCatalogue(): void {
    if (this.records.length === 0) {
      const empty = document.createElement("p");
      empty.className = "muted";
      empty.textContent = t("audit.empty");
      this.root.appendChild(empty);
      return;
    }
    const toolbar = document.createElement("div");
    toolbar.className = "audit-visual-toolbar";
    const title = document.createElement("strong");
    title.textContent = t("audit.allRuns");
    const expand = document.createElement("button");
    expand.type = "button";
    expand.className = "btn btn--small";
    expand.textContent = this.workspaceOpen ? t("audit.collapse") : t("audit.expand");
    expand.addEventListener("click", () => {
      this.workspaceOpen = !this.workspaceOpen;
      this.render();
    });
    toolbar.append(title, expand);
    this.root.appendChild(toolbar);
    const summary = document.createElement("div");
    summary.className = "audit-summary";
    for (const [label, value] of this.catalogueSummary()) {
      summary.appendChild(this.metric(label, value));
    }
    this.root.appendChild(summary);
    const grid = document.createElement("div");
    grid.className = "audit-workspace__grid audit-workspace__grid--catalogue";
    grid.append(this.renderTimeline(true), this.renderDetail());
    this.root.appendChild(grid);
  }

  private renderAuditHeader(): HTMLElement {
    const toolbar = document.createElement("div");
    toolbar.className = "audit-visual-toolbar";
    const title = document.createElement("strong");
    title.textContent = `${t("audit.run")} ${this.context!.run.id.slice(0, 8)} · ${localizeRunStatus(this.context!.run.status)}`;
    const expand = document.createElement("button");
    expand.type = "button";
    expand.className = "btn btn--small";
    expand.textContent = this.workspaceOpen ? t("audit.collapse") : t("audit.expand");
    expand.addEventListener("click", () => {
      this.workspaceOpen = !this.workspaceOpen;
      this.render();
    });
    toolbar.append(title, expand);
    return toolbar;
  }

  private renderSummary(): HTMLElement {
    const summary = document.createElement("div");
    summary.className = "audit-summary";
    const run = this.context!.run;
    const errors = this.records.filter((record) => record.category === "node_failed").length;
    const approvals = this.records.filter((record) => record.category === "approval").length;
    const denied = this.records.filter((record) => record.decision === "deny" || record.decision === "rejected").length;
    const duration = (run.finished_at_ms ?? Date.now()) - run.started_at_ms;
    for (const [label, value] of [
      [t("runs.status"), localizeRunStatus(run.status)],
      [t("runs.duration"), formatDuration(duration)],
      [t("runs.nodes"), String(run.nodes_executed)],
      [t("runs.events"), String(run.event_count)],
      [t("runs.artifacts"), String(run.artifact_count ?? 0)],
      [t("audit.approvals"), String(approvals)],
      [t("audit.denials"), String(denied)],
      [t("audit.failures"), String(errors)],
    ]) {
      summary.appendChild(this.metric(label, value));
    }
    return summary;
  }

  private metric(label: string, value: string): HTMLElement {
    const card = document.createElement("div");
    card.className = "audit-metric";
    const valueElement = document.createElement("strong");
    valueElement.textContent = value;
    const labelElement = document.createElement("span");
    labelElement.textContent = label;
    card.append(valueElement, labelElement);
    return card;
  }

  private renderGraph(): HTMLElement {
    const section = document.createElement("section");
    section.className = "audit-graph";
    const title = document.createElement("h4");
    title.textContent = t("audit.executionGraph");
    section.appendChild(title);
    const workflow = this.context!.workflow;
    const nodes = workflow.nodes;
    if (nodes.length === 0) {
      const empty = document.createElement("p");
      empty.className = "muted";
      empty.textContent = t("audit.noGraph");
      section.appendChild(empty);
      return section;
    }
    const xs = nodes.map((node) => node.position?.x ?? 0);
    const ys = nodes.map((node) => node.position?.y ?? 0);
    const minX = Math.min(...xs);
    const minY = Math.min(...ys);
    const width = Math.max(400, ...nodes.map((node) => (node.position?.x ?? 0) - minX + 180));
    const height = Math.max(240, ...nodes.map((node) => (node.position?.y ?? 0) - minY + 110));
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", `0 0 ${width} ${height}`);
    svg.classList.add("audit-graph__svg");
    const traversedEdges = new Set(
      this.context!.events
        .map((envelope) => envelope.event)
        .filter((event) => event.type === "edge_activated" || event.type === "data_transferred")
        .map((event) => (event as { edge_id: string }).edge_id),
    );
    for (const edge of workflow.edges) {
      const source = nodes.find((node) => node.id === edge.source);
      const target = nodes.find((node) => node.id === edge.target);
      if (!source || !target) continue;
      const x1 = (source.position?.x ?? 0) - minX + 150;
      const y1 = (source.position?.y ?? 0) - minY + 42;
      const x2 = (target.position?.x ?? 0) - minX;
      const y2 = (target.position?.y ?? 0) - minY + 42;
      const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
      path.setAttribute("d", `M ${x1} ${y1} C ${x1 + 40} ${y1}, ${x2 - 40} ${y2}, ${x2} ${y2}`);
      path.classList.add("audit-edge", `audit-edge--${edge.kind}`);
      if (traversedEdges.has(edge.id)) path.classList.add("is-traversed");
      svg.appendChild(path);
    }
    for (const node of nodes) {
      const state = this.nodeState(node.id);
      const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
      group.classList.add("audit-node", `audit-node--${state}`);
      if (node.id === this.selectedNode) group.classList.add("is-selected");
      group.setAttribute("transform", `translate(${(node.position?.x ?? 0) - minX}, ${(node.position?.y ?? 0) - minY})`);
      group.addEventListener("click", () => {
        this.selectedNode = node.id;
        this.selectedRecord = null;
        this.render();
      });
      const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
      rect.setAttribute("width", "150");
      rect.setAttribute("height", "84");
      rect.setAttribute("rx", "7");
      const label = document.createElementNS("http://www.w3.org/2000/svg", "text");
      label.setAttribute("x", "10");
      label.setAttribute("y", "22");
      label.classList.add("audit-node__label");
      label.textContent = node.label ?? node.id;
      const type = document.createElementNS("http://www.w3.org/2000/svg", "text");
      type.setAttribute("x", "10");
      type.setAttribute("y", "40");
      type.classList.add("audit-node__type");
      type.textContent = node.type;
      const status = document.createElementNS("http://www.w3.org/2000/svg", "text");
      status.setAttribute("x", "10");
      status.setAttribute("y", "66");
      status.classList.add("audit-node__status");
      status.textContent = t(`audit.nodeState.${state}`);
      group.append(rect, label, type, status);
      svg.appendChild(group);
    }
    section.appendChild(svg);
    return section;
  }

  private nodeState(nodeId: string): "pending" | "running" | "success" | "failed" {
    let state: "pending" | "running" | "success" | "failed" = "pending";
    for (const envelope of this.context!.events) {
      const event = envelope.event;
      if (!("node_id" in event) || event.node_id !== nodeId) continue;
      if (event.type === "node_started") state = "running";
      if (event.type === "node_finished") state = "success";
      if (event.type === "node_failed") state = "failed";
    }
    return state;
  }

  private renderTimeline(catalogue = false): HTMLElement {
    const section = document.createElement("section");
    section.className = "audit-timeline";
    const title = document.createElement("h4");
    title.textContent = catalogue ? t("audit.recentEvents") : t("audit.timeline");
    section.appendChild(title);
    const list = document.createElement("div");
    list.className = "audit-timeline__list";
    const records = this.selectedNode
      ? this.records.filter((record) => record.node_id === this.selectedNode)
      : this.records;
    for (const record of records.slice().reverse().slice(0, 300)) {
      const row = document.createElement("button");
      row.type = "button";
      row.className = `audit-timeline__row audit-timeline__row--${record.category.replace(/_/g, "-")}`;
      if (record.seq === this.selectedRecord?.seq) row.classList.add("is-selected");
      const time = document.createElement("time");
      time.textContent = new Date(record.timestamp_ms).toLocaleTimeString();
      const category = document.createElement("code");
      category.textContent = localizeAuditCategory(record.category);
      const message = document.createElement("span");
      message.textContent = record.message;
      row.append(time, category, message);
      row.addEventListener("click", () => {
        this.selectedRecord = record;
        this.selectedNode = record.node_id ?? null;
        this.render();
      });
      list.appendChild(row);
    }
    if (records.length === 0) {
      const empty = document.createElement("p");
      empty.className = "muted";
      empty.textContent = t("audit.empty");
      list.appendChild(empty);
    }
    section.appendChild(list);
    return section;
  }

  private renderDetail(): HTMLElement {
    const section = document.createElement("section");
    section.className = "audit-detail";
    const title = document.createElement("h4");
    title.textContent = t("audit.detail");
    section.appendChild(title);
    if (this.selectedRecord) {
      const record = this.selectedRecord;
      const lines: Array<[string, string]> = [
        [t("audit.time"), new Date(record.timestamp_ms).toLocaleString()],
        [t("audit.category"), localizeAuditCategory(record.category)],
        [t("audit.node"), record.node_id ?? "-"],
        [t("audit.capability"), record.capability ?? "-"],
        [t("audit.decision"), record.decision ?? "-"],
        [t("audit.message"), record.message],
      ];
      for (const [label, value] of lines) section.appendChild(this.detailLine(label, value));
      const raw = document.createElement("pre");
      raw.className = "audit-detail__raw";
      raw.textContent = JSON.stringify(record, null, 2);
      section.appendChild(raw);
      return section;
    }
    if (this.selectedNode) {
      const node = this.context!.workflow.nodes.find((candidate) => candidate.id === this.selectedNode);
      if (node) section.appendChild(this.detailLine(t("audit.node"), `${node.label ?? node.id} · ${node.type}`));
      const events = this.context!.events.filter((envelope) =>
        "node_id" in envelope.event && envelope.event.node_id === this.selectedNode
      );
      const raw = document.createElement("pre");
      raw.className = "audit-detail__raw";
      raw.textContent = JSON.stringify(events, null, 2);
      section.appendChild(raw);
      return section;
    }
    const hint = document.createElement("p");
    hint.className = "muted";
    hint.textContent = t("audit.selectHint");
    section.appendChild(hint);
    return section;
  }

  private detailLine(label: string, value: string): HTMLElement {
    const line = document.createElement("div");
    line.className = "audit-detail__line";
    const key = document.createElement("span");
    key.textContent = label;
    const text = document.createElement("span");
    text.textContent = value;
    line.append(key, text);
    return line;
  }

  private catalogueSummary(): Array<[string, string]> {
    const failures = this.records.filter((record) => record.category === "node_failed").length;
    const approvals = this.records.filter((record) => record.category === "approval").length;
    return [
      [t("audit.records"), String(this.records.length)],
      [t("audit.runs"), String(new Set(this.records.map((record) => record.run_id)).size)],
      [t("audit.approvals"), String(approvals)],
      [t("audit.failures"), String(failures)],
    ];
  }
}

function formatDuration(milliseconds: number): string {
  const value = Math.max(0, milliseconds);
  if (value < 1000) return `${Math.round(value)} ms`;
  if (value < 60_000) return `${(value / 1000).toFixed(1)} s`;
  return `${Math.floor(value / 60_000)}m ${Math.round((value % 60_000) / 1000)}s`;
}
