import { beforeEach, describe, expect, it } from "vitest";

import { Canvas } from "./canvas";
import { EventLog } from "./event-log";
import { emptyWorkflow } from "../model/workflow";
import { EventEnvelope, ExecutionEvent, NodeDescriptor } from "../runtime/types";

function descriptor(nodeType: string): NodeDescriptor {
  return {
    node_type: nodeType,
    display_name: nodeType,
    category: "Test",
    description: "",
    version: "",
    inputs: [{ name: "in", display_name: "In", kind: "input", value_type: "any", required: false }],
    outputs: [
      { name: "out", display_name: "Out", kind: "output", value_type: "any", required: false },
    ],
    config_schema: {},
    capabilities: [],
    permissions: [],
    dangerous: false,
    allows_additional_config: true,
  };
}

function envelope(seq: number, event: ExecutionEvent): EventEnvelope {
  return { run_id: "r1", seq, timestamp_ms: 1_700_000_000_000 + seq, event };
}

describe("run status visualisation", () => {
  let canvas: Canvas;
  let log: EventLog;

  beforeEach(() => {
    document.body.innerHTML = `
      <svg id="canvas"><g id="viewport"><g id="edges"></g><g id="nodes"></g><path id="pending-edge"></path></g></svg>
      <div id="events"></div>`;
    const workflow = emptyWorkflow();
    workflow.nodes.push({ id: "log", type: "core.Log", config: {} });
    const descriptors = new Map(
      ["core.Start", "core.Log", "core.End"].map((type) => [type, descriptor(type)]),
    );
    canvas = new Canvas(document.getElementById("canvas") as unknown as SVGSVGElement, workflow, {
      onChange: () => undefined,
      onSelect: () => undefined,
      onStatus: () => undefined,
      descriptorFor: (nodeType) => descriptors.get(nodeType),
    });
    canvas.render();
    log = new EventLog(document.getElementById("events")!, canvas);
  });

  function node(id: string): SVGGElement {
    return document.querySelector<SVGGElement>(`[data-node-id="${id}"]`)!;
  }

  it("highlights the executing node and then marks it done", () => {
    log.append(envelope(0, { type: "run_started", workflow_id: "wf" }));
    log.setRunStatus("running");

    log.append(
      envelope(1, { type: "node_started", node_id: "start", node_type: "core.Start" }),
    );
    expect(node("start").classList.contains("node--active")).toBe(true);
    expect(node("start").classList.contains("node--running")).toBe(true);

    log.append(
      envelope(2, { type: "node_finished", node_id: "start", outputs: {}, duration_ms: 3 }),
    );
    expect(node("start").classList.contains("node--active")).toBe(false);
    expect(node("start").classList.contains("node--done")).toBe(true);
  });

  it("renders image artifact previews from node outputs", () => {
    log = new EventLog(
      document.getElementById("events")!,
      canvas,
      (runId, artifactId) => `/artifacts/${runId}/${artifactId}`,
    );
    log.append(
      envelope(0, {
        type: "node_finished",
        node_id: "capture",
        outputs: {
          artifact: {
            id: "image-1",
            name: "desktop",
            content_type: "image/png",
            size: 128,
          },
        },
        duration_ms: 9,
      }),
    );

    const image = document.querySelector<HTMLImageElement>(".event-artifact__image")!;
    expect(image.src).toContain("/artifacts/r1/image-1");
    expect(document.querySelector(".event-artifact__caption")?.textContent).toContain("image/png");
  });

  it("renders an image when an artifact is logged as JSON", () => {
    log = new EventLog(
      document.getElementById("events")!,
      canvas,
      (runId, artifactId) => `/artifacts/${runId}/${artifactId}`,
    );
    log.append(
      envelope(0, {
        type: "log",
        level: "info",
        message: JSON.stringify({
          id: "image-2",
          name: "desktop",
          content_type: "image/png",
          size: 256,
        }),
      }),
    );

    const image = document.querySelector<HTMLImageElement>(".event-artifact__image")!;
    expect(image.src).toContain("/artifacts/r1/image-2");
    expect(document.querySelector(".event-artifact__caption")?.textContent).toContain("image/png");
  });

  it("shows a recoverable message when an artifact image cannot load", () => {
    log = new EventLog(
      document.getElementById("events")!,
      canvas,
      (runId, artifactId) => `/artifacts/${runId}/${artifactId}`,
    );
    log.append(
      envelope(0, {
        type: "node_finished",
        node_id: "capture",
        outputs: {
          artifact: {
            id: "image-broken",
            name: "desktop",
            content_type: "image/png",
            size: 128,
          },
        },
        duration_ms: 9,
      }),
    );

    const image = document.querySelector<HTMLImageElement>(".event-artifact__image")!;
    image.dispatchEvent(new Event("error"));
    expect(image.hidden).toBe(true);
    expect(document.querySelector(".event-artifact__error")?.textContent).toContain(
      "Preview unavailable",
    );
  });

  it("renders command stdout, stderr and exit metadata", () => {
    log.append(
      envelope(0, {
        type: "node_finished",
        node_id: "command",
        outputs: {
          out: "hello\n",
          stderr: "warning\n",
          exit_code: 0,
          success: true,
          pid: 1234,
        },
        duration_ms: 12,
      }),
    );

    const command = document.querySelector(".event-command")!;
    expect(command.textContent).toContain("Command output");
    expect(command.textContent).toContain("hello");
    expect(command.textContent).toContain("warning");
    expect(command.textContent).toContain("Exit code 0 · PID 1234");
  });

  it("marks a failed node", () => {
    log.append(
      envelope(0, {
        type: "node_failed",
        node_id: "log",
        code: "E_EXECUTION",
        message: "boom",
        retryable: false,
      }),
    );
    expect(node("log").classList.contains("node--failed")).toBe(true);
    expect(document.getElementById("events")!.textContent).toContain("boom");
  });

  it("clears the decoration when a new run starts", () => {
    log.append(
      envelope(0, { type: "node_finished", node_id: "log", outputs: {}, duration_ms: 1 }),
    );
    expect(node("log").classList.contains("node--done")).toBe(true);
    log.clear();
    log.setRunStatus("pending");
    expect(node("log").className).not.toContain("node--done");
  });

  it("renders each event type readably", () => {
    const events: ExecutionEvent[] = [
      { type: "run_started", workflow_id: "wf.demo" },
      { type: "node_progress", node_id: "ocr", progress: 0.5, message: "page 1" },
      { type: "capability_decision", capability: "windows.Input.Keyboard", decision: "require_approval" },
      { type: "log", level: "warn", message: "careful" },
      { type: "run_completed", nodes_executed: 3, duration_ms: 12 },
    ];
    events.forEach((event, index) => log.append(envelope(index, event)));

    const text = document.getElementById("events")!.textContent ?? "";
    expect(text).toContain("wf.demo");
    expect(text).toContain("50%");
    expect(text).toContain("windows.Input.Keyboard: require_approval");
    expect(text).toContain("careful");
    expect(text).toContain("3 node(s) in 12ms");
  });
});
