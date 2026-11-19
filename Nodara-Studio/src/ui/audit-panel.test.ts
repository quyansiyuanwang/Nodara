import { beforeEach, describe, expect, it } from "vitest";

import { AuditPanel } from "./audit-panel";
import { setLocale } from "../i18n";
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
  let panel: AuditPanel;
  beforeEach(() => {
    document.body.innerHTML = "";
    host = document.createElement("div");
    document.body.appendChild(host);
    setLocale("en");
    panel = new AuditPanel(host, (runId, id) => `/runs/${runId}/artifacts/${id}`);
  });

  it("summarizes records and shows a timeline", () => {
    panel.setData([
      record({ category: "capability_evaluated", node_id: "log", decision: "allow", message: "allowed" }),
      record({ seq: 1, category: "node_failed", node_id: "log", message: "boom" }),
    ]);
    expect(host.querySelectorAll(".audit-metric")).toHaveLength(4);
    expect(host.querySelectorAll(".audit-timeline__row")).toHaveLength(2);
    expect(host.textContent).toContain("boom");
  });

  it("renders a workflow graph and highlights traversed edges", () => {
    panel.setData([record({ category: "node_finished", node_id: "start", message: "done" })], {
      run: run(),
      workflow: workflow(),
      events: [event("start", "node_started"), event("start", "node_finished"), event("start", "edge_activated")],
    });
    expect(host.querySelectorAll(".audit-node")).toHaveLength(2);
    expect(host.querySelector(".audit-node--success")).not.toBeNull();
    expect(host.querySelector(".audit-edge.is-traversed")).not.toBeNull();
    expect(host.querySelectorAll(".audit-metric")).toHaveLength(8);
  });

  it("opens event details when a timeline row is selected", () => {
    panel.setData([record({ message: "first", decision: "allow", capability: "cap", node_id: "log" })]);
    host.querySelector<HTMLButtonElement>(".audit-timeline__row")!.click();
    expect(host.querySelector(".audit-detail")?.textContent).toContain("first");
    expect(host.querySelector(".audit-detail")?.textContent).toContain("cap");
  });

  it("opens the full audit workspace", () => {
    panel.setData([], { run: run(), workflow: workflow(), events: [] });
    host.querySelector<HTMLButtonElement>(".audit-visual-toolbar button")!.click();
    expect(host.classList.contains("audit--workspace")).toBe(true);
  });

  it("replaces the contents on refresh", () => {
    panel.setData([record({ message: "first" })]);
    panel.setData([record({ message: "second" })]);
    expect(host.textContent).toContain("second");
    expect(host.textContent).not.toContain("first");
  });

  const mail = { id: "a-1", name: "inventory.eml", content_type: "message/rfc822", size: 250 };
  function mailEvents(outputs: Record<string, unknown>): EventEnvelope[] {
    return [
      { type: "node_started", node_id: "log", node_type: "mail.Digest", input: {
        inputs: { digest: "PNG (1)\nScreenshot <1>.png" }, config: {}, resolved_config: {}, variables_before: {},
      } },
      { type: "node_finished", node_id: "log", duration_ms: 1, outputs },
    ].map((event, seq) => ({ run_id: run().id, timestamp_ms: 1, seq, event } as EventEnvelope));
  }
  function mailContext(events: EventEnvelope[]) {
    const wf = workflow();
    wf.nodes[1].type = "mail.Digest";
    wf.nodes[1].config = { delivery: "smtp" };
    return { run: run(), workflow: wf, events };
  }

  it.each([
    ["preview", "Local preview · Not sent"], ["smtp_accepted", "Accepted by SMTP relay"], [undefined, "Email file"],
  ])("shows only what the plugin recorded for delivery %s", (delivery, label) => {
    const context = mailContext(mailEvents({ delivery, message: mail }));
    panel.setData([], context);
    expect(host.querySelector(".audit-receipt > span")?.textContent).toBe(label);
    expect(host.querySelector(".audit-mail-body")?.textContent).toBe("PNG (1)\nScreenshot <1>.png");
    const link = host.querySelector<HTMLAnchorElement>(".mail-download")!;
    expect(link.getAttribute("href")).toBe(`/runs/${context.run.id}/artifacts/a-1`);
    expect(link.download).toBe(mail.name);
  });

  it("does not carry over a receipt to a failed attempt, missing file, or another run", () => {
    const context = mailContext(mailEvents({ delivery: "preview", message: mail }));
    setLocale("zh-CN");
    panel.setData([], context);
    expect(host.textContent).toContain("本地预览 · 未发送");
    const failed: EventEnvelope = { ...context.events[0], event: { type: "node_failed", node_id: "log", code: "deny", message: "denied", retryable: false } };
    for (const events of [[...context.events, context.events[0]], [...context.events, failed], mailEvents({ delivery: "preview" }), []]) {
      panel.setData([], { ...context, events });
      expect(host.querySelector(".audit-receipt")).toBeNull();
    }
    panel.setData([], { ...context, run: { ...context.run, id: "another" } });
    expect(host.querySelector(".audit-receipt")).toBeNull();
    setLocale("en");
  });

  it("keeps record selection separate from graph filtering and clears stale details", () => {
    const records = [record({ node_id: "start" }), record({ seq: 1, node_id: "log", message: "result" })];
    panel.setData(records, { run: run(), workflow: workflow(), events: [] });
    const row = host.querySelector<HTMLButtonElement>(".audit-timeline__row")!;
    row.focus();
    row.click();
    expect(document.activeElement?.classList.contains("is-selected")).toBe(true);
    expect(host.querySelectorAll(".audit-timeline__row")).toHaveLength(2);
    host.querySelector(".audit-node")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    expect(host.querySelectorAll(".audit-timeline__row")).toHaveLength(1);
    host.querySelector<HTMLButtonElement>(".audit-filter-clear")!.click();
    expect(host.querySelectorAll(".audit-timeline__row")).toHaveLength(2);
    host.querySelector<HTMLButtonElement>(".audit-timeline__row")!.click();
    panel.setData([record({ run_id: "other", seq: 1 })]);
    expect(host.querySelector(".audit-detail__raw")).toBeNull();
  });
});
