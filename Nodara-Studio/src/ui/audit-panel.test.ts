import { beforeEach, describe, expect, it } from "vitest";

import { AuditPanel } from "./audit-panel";
import { AuditRecord, EventEnvelope, RunSnapshot, Workflow } from "../runtime/types";

function record(overrides: Partial<AuditRecord>): AuditRecord {
  return {
    seq: 0,
    timestamp_ms: 1_700_000_000_000,
    run_id: "run-12345678",
    category: "capability_evaluated",
    message: "",
    detail: null,
    ...overrides,
  };
}

function workflow(): Workflow {
  return {
    schema_version: "2.1",
    id: "wf.audit",
    metadata: { name: "Audit", tags: [] },
    nodes: [
      { id: "start", type: "core.Start", label: "Start", config: {}, position: { x: 0, y: 0 } },
      { id: "log", type: "core.Log", label: "Log", config: {}, position: { x: 240, y: 0 } },
    ],
    edges: [{ id: "e1", kind: "control", source: "start", target: "log" }],
    variables: {},
  };
}

function run(): RunSnapshot {
  return {
    id: "run-12345678",
    workflow_id: "wf.audit",
    status: "completed",
    started_at_ms: 1_700_000_000_000,
    finished_at_ms: 1_700_000_000_500,
    nodes_executed: 2,
    variables: {},
    event_count: 3,
    artifact_count: 0,
  };
}

function event(nodeId: string, type: EventEnvelope["event"]["type"]): EventEnvelope {
  const payload = type === "node_finished"
    ? { type, node_id: nodeId, outputs: {}, duration_ms: 10 }
    : type === "node_started"
      ? { type, node_id: nodeId, node_type: "core.Log" }
      : { type, edge_id: "e1", source: "start", target: "log", branch: "always" };
  return { run_id: "run-12345678", seq: 0, timestamp_ms: 1, event: payload } as EventEnvelope;
}

describe("graphical audit view", () => {
  let host: HTMLElement;
  beforeEach(() => {
    document.body.innerHTML = "";
    host = document.createElement("div");
    document.body.appendChild(host);
  });

  it("summarizes records and shows a timeline", () => {
    const panel = new AuditPanel(host);
    panel.setRecords([
      record({ category: "capability_evaluated", node_id: "log", decision: "allow", message: "allowed" }),
      record({ seq: 1, category: "node_failed", node_id: "log", message: "boom" }),
    ]);
    expect(host.querySelectorAll(".audit-metric")).toHaveLength(4);
    expect(host.querySelectorAll(".audit-timeline__row")).toHaveLength(2);
    expect(host.textContent).toContain("boom");
  });

  it("renders a workflow graph and highlights traversed edges", () => {
    const panel = new AuditPanel(host);
    panel.setContext({
      run: run(),
      workflow: workflow(),
      events: [event("start", "node_started"), event("start", "node_finished"), event("start", "edge_activated")],
    });
    panel.setRecords([record({ category: "node_finished", node_id: "start", message: "done" })]);
    expect(host.querySelectorAll(".audit-node")).toHaveLength(2);
    expect(host.querySelector(".audit-node--success")).not.toBeNull();
    expect(host.querySelector(".audit-edge.is-traversed")).not.toBeNull();
    expect(host.querySelectorAll(".audit-metric")).toHaveLength(8);
  });

  it("opens event details when a timeline row is selected", () => {
    const panel = new AuditPanel(host);
    panel.setRecords([record({ message: "first", decision: "allow", capability: "cap", node_id: "log" })]);
    host.querySelector<HTMLButtonElement>(".audit-timeline__row")!.click();
    expect(host.querySelector(".audit-detail")?.textContent).toContain("first");
    expect(host.querySelector(".audit-detail")?.textContent).toContain("cap");
  });

  it("opens the full audit workspace", () => {
    const panel = new AuditPanel(host);
    panel.setContext({ run: run(), workflow: workflow(), events: [] });
    host.querySelector<HTMLButtonElement>(".audit-visual-toolbar button")!.click();
    expect(host.classList.contains("audit--workspace")).toBe(true);
  });

  it("replaces the contents on refresh", () => {
    const panel = new AuditPanel(host);
    panel.setRecords([record({ message: "first" })]);
    panel.setRecords([record({ message: "second" })]);
    expect(host.textContent).toContain("second");
    expect(host.textContent).not.toContain("first");
  });
});
