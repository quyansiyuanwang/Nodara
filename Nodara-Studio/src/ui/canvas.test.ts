import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { Canvas } from "./canvas";
import { emptyWorkflow } from "../model/workflow";
import { NodeDescriptor, Workflow } from "../runtime/types";

function descriptor(nodeType: string): NodeDescriptor {
  const name = nodeType.split(".").pop() ?? nodeType;
  return {
    node_type: nodeType,
    display_name: name,
    category: "Test",
    description: "",
    version: "",
    inputs: [
      { name: "in", display_name: "In", kind: "input", value_type: "any", required: false },
    ],
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

const activeCanvases: Canvas[] = [];

interface Harness {
  canvas: Canvas;
  workflow: Workflow;
  changes: () => number;
  descriptors: Map<string, NodeDescriptor>;
}

function harness(): Harness {
  document.body.innerHTML = `
    <svg id="canvas">
      <g id="viewport">
        <g id="edges"></g>
        <g id="nodes"></g>
        <path id="pending-edge"></path>
      </g>
    </svg>`;
  const workflow = emptyWorkflow();
  let changes = 0;
  let canvas: Canvas;
  const descriptors = new Map(
    ["core.Start", "core.Log", "core.End"].map((type) => [type, descriptor(type)]),
  );
  canvas = new Canvas(document.getElementById("canvas") as unknown as SVGSVGElement, workflow, {
    onChange: () => {
      changes += 1;
      // Mirror the application: a document change re-renders the canvas.
      canvas.render();
    },
    onSelect: () => undefined,
    onStatus: () => undefined,
    descriptorFor: (nodeType) => descriptors.get(nodeType),
  });
  canvas.render();
  activeCanvases.push(canvas);
  return { canvas, workflow, changes: () => changes, descriptors };
}

function pointerDown(target: Element, x = 0, y = 0) {
  target.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, clientX: x, clientY: y }));
}
function pointerUp(target: Element, x = 0, y = 0) {
  target.dispatchEvent(new MouseEvent("pointerup", { bubbles: true, clientX: x, clientY: y }));
}
function pointerEvent(type: string, x = 0, y = 0): PointerEvent {
  return new MouseEvent(type, {
    bubbles: true,
    button: 0,
    clientX: x,
    clientY: y,
  }) as unknown as PointerEvent;
}

describe("graph editing on the canvas", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  afterEach(() => {
    for (const canvas of activeCanvases.splice(0)) canvas.destroy();
  });

  it("refuses to add a second Start node", () => {
    const { canvas, workflow } = harness();
    canvas.addNode(descriptor("core.Start"), 300, 200);
    expect(workflow.nodes.filter((node) => node.type === "core.Start")).toHaveLength(1);
  });

  it("refuses to duplicate a Start node", () => {
    const { canvas, workflow, changes } = harness();
    canvas.select("start");
    canvas.render();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "d", ctrlKey: true }));
    expect(workflow.nodes.filter((node) => node.type === "core.Start")).toHaveLength(1);
    expect(changes()).toBe(0);

    const start = document.querySelector<SVGGElement>('[data-node-id="start"]')!;
    start.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 80, clientY: 80 }));
    const duplicate = document.querySelector<HTMLButtonElement>(
      '.context-menu__item[data-action="duplicate"]'
    )!;
    expect(duplicate.disabled).toBe(true);
  });

  it("adds a node from the palette", () => {
    const { canvas, workflow } = harness();
    const before = workflow.nodes.length;
    canvas.addNode(descriptor("core.Log"), 300, 200);

    expect(workflow.nodes).toHaveLength(before + 1);
    const added = workflow.nodes[workflow.nodes.length - 1];
    expect(added.type).toBe("core.Log");
    // Drops are offset by the node count so two nodes never land exactly on top
    // of each other.
    expect(added.position).toEqual({ x: 300 + before * 12, y: 200 + before * 12 });
    expect(document.querySelectorAll(".node")).toHaveLength(before + 1);
  });

  it("drops a palette item at the pointer position", () => {
    const { canvas, workflow } = harness();
    const svg = document.getElementById("canvas") as unknown as SVGSVGElement;
    svg.getBoundingClientRect = () => ({
      left: 100,
      top: 50,
      right: 700,
      bottom: 550,
      width: 600,
      height: 500,
      x: 100,
      y: 50,
      toJSON: () => ({}),
    });

    const source = document.createElement("button");
    source.addEventListener("pointerdown", (event) =>
      canvas.beginPaletteDrag(descriptor("core.Log"), event as PointerEvent),
    );
    document.body.appendChild(source);
    source.dispatchEvent(pointerEvent("pointerdown", 120, 70));
    window.dispatchEvent(pointerEvent("pointermove", 300, 250));
    expect(document.body.classList.contains("is-palette-dragging")).toBe(true);

    window.dispatchEvent(pointerEvent("pointerup", 300, 250));
    const added = workflow.nodes[workflow.nodes.length - 1];
    expect(added.type).toBe("core.Log");
    expect(added.position).toEqual({ x: 100, y: 146 });
    expect(document.body.classList.contains("is-palette-dragging")).toBe(false);
  });

  it("does not leave a selection state behind after dragging a node", () => {
    const { canvas } = harness();
    canvas.addNode(descriptor("core.Log"), 200, 100);
    canvas.render();

    const node = document.querySelector<SVGGElement>('[data-node-id="log"]')!;
    node.dispatchEvent(pointerEvent("pointerdown", 100, 100));
    window.dispatchEvent(pointerEvent("pointermove", 120, 130));
    expect(document.body.classList.contains("is-canvas-dragging")).toBe(true);

    window.dispatchEvent(pointerEvent("pointerup", 120, 130));
    expect(document.body.classList.contains("is-canvas-dragging")).toBe(false);
  });

  it("styles and labels success and failure branches", () => {
    const { canvas, workflow } = harness();
    workflow.edges.push({
      id: "failure-path",
      kind: "control",
      source: "start",
      target: "end",
      branch: "failure",
    });
    canvas.render();

    const edge = document.querySelector<SVGPathElement>(".edge")!;
    expect(edge.classList.contains("edge--failure")).toBe(true);
    expect(document.querySelector(".edge__label")?.textContent).toBe("Failure");
  });

  it("bends a tall connection so its arrow follows the approach", () => {
    const { canvas, workflow } = harness();
    workflow.edges.push({ id: "start-end", kind: "control", source: "start", target: "end" });
    workflow.nodes[1].position = { x: 720, y: 660 };
    canvas.render();

    const path = document.querySelector<SVGPathElement>(".edge")!;
    const numbers = path
      .getAttribute("d")!
      .match(/-?\d+(?:\.\d+)?/g)!
      .map(Number);
    const control2Y = numbers[5];
    const targetY = numbers[7];
    expect(control2Y).not.toBe(targetY);
  });

  it("lays out a graph in topological columns", () => {
    const { canvas, workflow } = harness();
    canvas.addNode(descriptor("core.Log"), 500, 500);
    canvas.addNode(descriptor("core.Log"), 200, 700);
    const logs = workflow.nodes.filter((node) => node.type === "core.Log");
    workflow.edges.push(
      { id: "e1", kind: "control", source: "start", target: logs[0].id },
      { id: "e2", kind: "control", source: logs[0].id, target: logs[1].id },
      { id: "e3", kind: "control", source: logs[1].id, target: "end" },
    );
    workflow.nodes.forEach((node) => { node.position = { x: 900, y: 900 }; });
    canvas.autoLayout();

    const start = workflow.nodes.find((node) => node.id === "start")!;
    const end = workflow.nodes.find((node) => node.id === "end")!;
    expect(start.position!.x).toBeLessThan(logs[0].position!.x);
    expect(logs[0].position!.x).toBeLessThan(logs[1].position!.x);
    expect(logs[1].position!.x).toBeLessThan(end.position!.x);
  });

  it("zooms, resets and fits the canvas view", () => {
    const { canvas, workflow } = harness();
    const svg = document.getElementById("canvas") as unknown as SVGSVGElement;
    svg.getBoundingClientRect = () => ({
      left: 0,
      top: 0,
      right: 800,
      bottom: 600,
      width: 800,
      height: 600,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    });
    workflow.nodes[0].position = { x: 50, y: 50 };
    workflow.nodes[1].position = { x: 900, y: 700 };
    canvas.render();

    canvas.zoomIn();
    expect(canvas.currentScale()).toBeGreaterThan(1);
    canvas.resetView();
    expect(canvas.currentScale()).toBe(1);
    canvas.fitToContent();
    expect(canvas.currentScale()).toBeLessThan(1);
    expect(canvas.currentScale()).toBeGreaterThanOrEqual(0.2);
  });

  it("connects an output port to an input port", () => {
    const { canvas, workflow, changes } = harness();
    canvas.addNode(descriptor("core.Log"), 200, 100);
    canvas.render();

    const outputs = document.querySelectorAll(".port--output");
    const inputs = document.querySelectorAll(".port--input");
    expect(outputs.length).toBe(3);
    expect(inputs.length).toBe(3);

    // Connect `start` (node 0, output) to the new log node (last, input).
    pointerDown(outputs[0]);
    pointerUp(inputs[inputs.length - 1]);

    expect(workflow.edges).toHaveLength(1);
    expect(workflow.edges[0].source).toBe("start");
    expect(workflow.edges[0].target).toBe("log");
    expect(workflow.edges[0].kind).toBe("data");
    expect(workflow.edges[0].source_port).toBe("out");
    expect(workflow.edges[0].target_port).toBe("in");
    expect(changes()).toBeGreaterThan(0);
    expect(document.querySelectorAll(".edge")).toHaveLength(1);
  });

  it("refuses a duplicate connection", () => {
    const { canvas, workflow } = harness();
    canvas.addNode(descriptor("core.Log"), 200, 100);
    canvas.render();
    const outputs = document.querySelectorAll(".port--output");
    const inputs = document.querySelectorAll(".port--input");

    pointerDown(outputs[0]);
    pointerUp(inputs[inputs.length - 1]);
    pointerDown(outputs[0]);
    pointerUp(inputs[inputs.length - 1]);

    expect(workflow.edges).toHaveLength(1);
  });

  it("refuses incompatible port types", () => {
    const { canvas, workflow, descriptors } = harness();
    const text = descriptor("test.Text");
    text.outputs = [
      { name: "out", display_name: "Text", kind: "output", value_type: "string", required: false },
    ];
    const number = descriptor("test.Number");
    number.inputs = [
      { name: "in", display_name: "Value", kind: "input", value_type: "number", required: false },
    ];
    descriptors.set(text.node_type, text);
    descriptors.set(number.node_type, number);
    canvas.addNode(text, 200, 100);
    canvas.addNode(number, 500, 100);
    canvas.render();

    const output = document.querySelector<SVGCircleElement>(
      '[data-node-id="text"] .port--output',
    )!;
    const input = document.querySelector<SVGCircleElement>(
      '[data-node-id="number"] .port--input',
    )!;
    pointerDown(output);
    pointerUp(input);

    expect(workflow.edges).toHaveLength(0);
  });

  it("connects ports by clicking output then input", () => {
    const { canvas, workflow } = harness();
    canvas.addNode(descriptor("core.Log"), 200, 100);
    canvas.render();
    const outputs = document.querySelectorAll<SVGCircleElement>(".port--output");
    const inputs = document.querySelectorAll<SVGCircleElement>(".port--input");

    outputs[0].dispatchEvent(new MouseEvent("click", { bubbles: true, button: 0 }));
    inputs[inputs.length - 1].dispatchEvent(
      new MouseEvent("pointerdown", { bubbles: true, button: 0 }),
    );

    expect(workflow.edges).toHaveLength(1);
  });

  it("deletes the selected node and its edges", () => {
    const { canvas, workflow } = harness();
    canvas.addNode(descriptor("core.Log"), 200, 100);
    canvas.render();
    const outputs = document.querySelectorAll(".port--output");
    const inputs = document.querySelectorAll(".port--input");
    pointerDown(outputs[0]);
    pointerUp(inputs[inputs.length - 1]);
    expect(workflow.edges).toHaveLength(1);

    // Select the log node, then press Delete.
    const node = document.querySelector<SVGGElement>('[data-node-id="log"]')!;
    pointerDown(node);
    pointerUp(node);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Delete" }));

    expect(workflow.nodes.some((candidate) => candidate.id === "log")).toBe(false);
    expect(workflow.edges).toHaveLength(0);
    expect(document.querySelectorAll(".node")).toHaveLength(2);
  });

  it("deletes the selected edge through its wide hit target", () => {
    const { canvas, workflow } = harness();
    canvas.addNode(descriptor("core.Log"), 200, 100);
    canvas.render();
    const outputs = document.querySelectorAll(".port--output");
    const inputs = document.querySelectorAll(".port--input");
    pointerDown(outputs[0]);
    pointerUp(inputs[inputs.length - 1]);

    const edge = document.querySelector<SVGPathElement>(".edge-hit")!;
    edge.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true }));
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Delete" }));

    expect(workflow.edges).toHaveLength(0);
  });

  it("deletes a connection from its context menu", () => {
    const { canvas, workflow } = harness();
    canvas.addNode(descriptor("core.Log"), 200, 100);
    canvas.render();
    const outputs = document.querySelectorAll(".port--output");
    const inputs = document.querySelectorAll(".port--input");
    pointerDown(outputs[0]);
    pointerUp(inputs[inputs.length - 1]);

    const hit = document.querySelector<SVGPathElement>(".edge-hit")!;
    hit.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 80, clientY: 80 }));
    const remove = document.querySelector<HTMLButtonElement>(".context-menu__item--danger")!;
    expect(remove.textContent).toContain("Delete connection");
    remove.click();

    expect(workflow.edges).toHaveLength(0);
  });

  it("changes connection branches from the edge context menu", () => {
    const { canvas, workflow } = harness();
    canvas.addNode(descriptor("core.Log"), 200, 100);
    canvas.render();
    const output = document.querySelector<SVGElement>(".exec-port--always")!;
    const input = document.querySelector<SVGElement>('[data-node-id="log"] .exec-port--input')!;
    pointerDown(output);
    pointerUp(input);

    const hit = document.querySelector<SVGPathElement>(".edge-hit")!;
    hit.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 80, clientY: 80 }));
    const failure = document.querySelector<HTMLButtonElement>(
      '.context-menu__item[data-action="branch-failure"]',
    )!;
    expect(failure.textContent).toContain("Failure");
    failure.click();
    expect(workflow.edges[0].branch).toBe("failure");

    hit.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 80, clientY: 80 }));
    document
      .querySelector<HTMLButtonElement>('.context-menu__item[data-action="branch-always"]')!
      .click();
    expect(workflow.edges[0].branch).toBeUndefined();
  });

  it("toggles a node from its context menu", () => {
    const { canvas, workflow } = harness();
    canvas.addNode(descriptor("core.Log"), 200, 100);
    canvas.render();

    const node = document.querySelector<SVGGElement>('[data-node-id="log"]')!;
    node.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 80, clientY: 80 }));
    const toggle = document.querySelector<HTMLButtonElement>(".context-menu__item[data-action=\"toggle\"]")!;
    expect(toggle.textContent).toContain("Disable node");
    toggle.click();

    expect(workflow.nodes.find((candidate) => candidate.id === "log")?.enabled).toBe(false);
    expect(document.querySelector<SVGGElement>('[data-node-id="log"]')?.classList.contains("node--disabled")).toBe(true);
  });

  it("toggles a breakpoint from the node context menu", () => {
    const { canvas, workflow } = harness();
    canvas.addNode(descriptor("core.Log"), 200, 100);
    canvas.render();

    let node = document.querySelector<SVGGElement>('[data-node-id="log"]')!;
    node.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 80, clientY: 80 }));
    const breakpoint = document.querySelector<HTMLButtonElement>(
      '.context-menu__item[data-action="breakpoint"]',
    )!;
    expect(breakpoint.textContent).toContain("Set breakpoint before node");
    breakpoint.click();

    expect(workflow.nodes.find((candidate) => candidate.id === "log")?.breakpoint).toBe(true);
    node = document.querySelector<SVGGElement>('[data-node-id="log"]')!;
    expect(node.classList.contains("node--breakpoint")).toBe(true);
    expect(node.querySelector(".node__breakpoint")).not.toBeNull();

    node.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 80, clientY: 80 }));
    const clear = document.querySelector<HTMLButtonElement>(
      '.context-menu__item[data-action="breakpoint"]',
    )!;
    expect(clear.textContent).toContain("Clear breakpoint");
    clear.click();
    expect(workflow.nodes.find((candidate) => candidate.id === "log")?.breakpoint).toBeUndefined();
  });

  it("toggles a breakpoint with F9", () => {
    const { canvas, workflow } = harness();
    canvas.addNode(descriptor("core.Log"), 200, 100);
    canvas.select("log");

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "F9" }));
    expect(workflow.nodes.find((candidate) => candidate.id === "log")?.breakpoint).toBe(true);

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "F9" }));
    expect(workflow.nodes.find((candidate) => candidate.id === "log")?.breakpoint).toBeUndefined();
  });

  it("duplicates a configured node with Ctrl+D", () => {
    const { canvas, workflow } = harness();
    canvas.addNode(descriptor("core.Log"), 200, 100);
    canvas.render();
    const source = workflow.nodes.find((node) => node.type === "core.Log")!;
    source.config = { message: "copied" };
    source.retry = 2;
    source.delay_before_ms = 15;
    source.breakpoint = true;
    canvas.select(source.id);
    canvas.render();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "d", ctrlKey: true }));

    const logs = workflow.nodes.filter((node) => node.type === "core.Log");
    expect(logs).toHaveLength(2);
    const copy = logs[1];
    expect(copy.id).not.toBe(source.id);
    expect(copy.config).toEqual({ message: "copied" });
    expect(copy.config).not.toBe(source.config);
    expect(copy.retry).toBe(2);
    expect(copy.delay_before_ms).toBe(15);
    expect(copy.breakpoint).toBe(true);
    expect(copy.position).toEqual({ x: 252, y: 152 });
  });

  it("deletes a node from its context menu", () => {
    const { canvas, workflow } = harness();
    canvas.addNode(descriptor("core.Log"), 200, 100);
    canvas.render();

    const node = document.querySelector<SVGGElement>('[data-node-id="log"]')!;
    node.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 80, clientY: 80 }));
    const remove = document.querySelector<HTMLButtonElement>(".context-menu__item--danger")!;
    expect(remove.textContent).toContain("Delete node");
    remove.click();

    expect(workflow.nodes.some((candidate) => candidate.id === "log")).toBe(false);
  });

  it("focus selects and centres a node on the canvas", () => {
    const { canvas } = harness();
    canvas.addNode(descriptor("core.Log"), 600, 400);
    const svg = document.getElementById("canvas") as unknown as SVGSVGElement;
    svg.getBoundingClientRect = () => ({
      left: 0,
      top: 0,
      right: 800,
      bottom: 600,
      width: 800,
      height: 600,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    });

    canvas.focus("log");

    expect(canvas.selectedNodeId()).toBe("log");
    expect(document.querySelector('[data-node-id="log"]')?.classList.contains("node--selected")).toBe(true);
    expect(document.getElementById("viewport")?.getAttribute("transform")).not.toBe(
      "translate(0 0) scale(1)",
    );
  });
  it("creates control edges without data ports and data edges with explicit ports", () => {
    const { canvas, workflow } = harness();
    canvas.addNode(descriptor("core.Log"), 320, 100);
    canvas.render();

    const dataOut = document.querySelector<SVGElement>('[data-node-id="start"] .port--output')!;
    const dataIn = document.querySelector<SVGElement>('[data-node-id="log"] .port--input')!;
    pointerDown(dataOut);
    pointerUp(dataIn);
    const controlOut = document.querySelector<SVGElement>('[data-node-id="log"] .exec-port--success')!;
    const controlIn = document.querySelector<SVGElement>('[data-node-id="end"] .exec-port--input')!;
    pointerDown(controlOut);
    pointerUp(controlIn);

    expect(workflow.edges).toHaveLength(2);
    const data = workflow.edges.find((edge) => edge.kind === "data")!;
    const control = workflow.edges.find((edge) => edge.kind === "control")!;
    expect(data.source_port).toBe("out");
    expect(data.target_port).toBe("in");
    expect(control.source_port).toBeUndefined();
    expect(control.target_port).toBeUndefined();
    expect(control.branch).toBe("success");
  });

  it("supports ctrl multi-selection and moves every selected node", () => {
    const { canvas, workflow } = harness();
    canvas.addNode(descriptor("core.Log"), 320, 80);
    canvas.addNode(descriptor("core.Log"), 320, 260);
    canvas.render();
    canvas.select(null);
    const start = document.querySelector<SVGGElement>('[data-node-id="start"]')!;
    const logs = [...document.querySelectorAll<SVGGElement>('[data-node-id^="log"]')];
    start.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, ctrlKey: true, clientX: 100, clientY: 100 }));
    window.dispatchEvent(new MouseEvent("pointerup"));
    logs[0].dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, ctrlKey: true, clientX: 350, clientY: 120 }));
    window.dispatchEvent(new MouseEvent("pointerup"));
    const selectedIds = canvas.selectedNodeIds();
    expect(selectedIds).toHaveLength(2);
    expect(selectedIds).toContain("start");
    expect(selectedIds).toContain(logs[0].dataset.nodeId);

    const beforeStart = { ...workflow.nodes.find((node) => node.id === "start")!.position! };
    const beforeLog = { ...workflow.nodes.find((node) => node.id === logs[0].dataset.nodeId)!.position! };
    logs[0].dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, clientX: 350, clientY: 120 }));
    window.dispatchEvent(new MouseEvent("pointermove", { bubbles: true, clientX: 390, clientY: 150 }));
    window.dispatchEvent(new MouseEvent("pointerup"));
    expect(workflow.nodes.find((node) => node.id === "start")!.position!.x).toBeGreaterThan(beforeStart.x);
    expect(workflow.nodes.find((node) => node.id === logs[0].dataset.nodeId)!.position!.x).toBeGreaterThan(beforeLog.x);
  });

  it("uses shift to select all nodes on directed control paths", () => {
    const { canvas, workflow } = harness();
    workflow.edges.push(
      { id: "s-a", kind: "control", source: "start", target: "end" },
    );
    const isolated = descriptor("core.Log");
    canvas.addNode(isolated, 20, 500);
    canvas.render();
    canvas.select("start");
    const end = document.querySelector<SVGGElement>('[data-node-id="end"]')!;
    end.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, shiftKey: true, clientX: 750, clientY: 180 }));
    window.dispatchEvent(new MouseEvent("pointerup"));
    expect(canvas.selectedNodeIds()).toEqual(["start", "end"]);

    const isolatedNode = document.querySelector<SVGGElement>('[data-node-id="log"]')!;
    isolatedNode.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, shiftKey: true, clientX: 40, clientY: 520 }));
    window.dispatchEvent(new MouseEvent("pointerup"));
    expect(canvas.selectedNodeIds()).toEqual(["start", "end"]);
  });

  it("box-selects intersected nodes and applies mixed quick settings to all", () => {
    const { canvas, workflow } = harness();
    canvas.addNode(descriptor("core.Log"), 300, 500);
    canvas.addNode(descriptor("core.Log"), 560, 500);
    canvas.render();
    workflow.nodes.find((node) => node.id === "log")!.retry = 2;
    canvas.render();
    const svg = document.getElementById("canvas") as unknown as SVGSVGElement;
    svg.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, clientX: 280, clientY: 470 }));
    window.dispatchEvent(new MouseEvent("pointermove", { bubbles: true, clientX: 760, clientY: 660 }));
    window.dispatchEvent(new MouseEvent("pointerup"));
    expect(canvas.selectedNodeIds()).toHaveLength(2);

    const retry = document.querySelector<HTMLInputElement>(
      '.node-quick-config input[data-field="retry"]',
    )!;
    expect(retry.placeholder).toBe("Multiple values");
    retry.value = "4";
    retry.dispatchEvent(new Event("change", { bubbles: true }));
    expect(workflow.nodes.filter((node) => node.id.startsWith("log")).every((node) => node.retry === 4)).toBe(true);
  });

  it("keeps an active edge decorated until states are cleared", () => {
    const { canvas, workflow } = harness();
    workflow.edges.push({ id: "s-e", kind: "control", source: "start", target: "end" });
    canvas.render();
    canvas.setEdgeState("s-e", "active");
    const group = document.querySelector('.edge-group[data-edge-id="s-e"]');
    expect(group?.querySelector(".edge--active")).not.toBeNull();
    expect(group?.querySelector(".edge-flow")).not.toBeNull();
    expect(group?.querySelector(".edge-runner")).not.toBeNull();
    canvas.clearStates();
    expect(group?.querySelector(".edge--active")).toBeNull();
    expect(group?.querySelector(".edge-flow")).toBeNull();
    expect(group?.querySelector(".edge-runner")).toBeNull();
  });

  it("does not pan when the pointer merely reaches the viewport edge", () => {
    harness();
    const viewport = document.getElementById("viewport")!;
    const before = viewport.getAttribute("transform");
    window.dispatchEvent(new MouseEvent("pointermove", { bubbles: true, clientX: 9999, clientY: 9999 }));
    expect(viewport.getAttribute("transform")).toBe(before);
  });
});
