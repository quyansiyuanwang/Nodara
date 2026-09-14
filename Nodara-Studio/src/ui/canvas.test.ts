import { beforeEach, describe, expect, it } from "vitest";

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

interface Harness {
  canvas: Canvas;
  workflow: Workflow;
  changes: () => number;
  descriptors: Map<string, NodeDescriptor>;
}

function harness(): Harness {
  document.body.innerHTML = `
    <svg id="canvas">
      <g id="edges"></g>
      <g id="nodes"></g>
      <path id="pending-edge"></path>
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
    expect(added.position).toEqual({ x: 116, y: 172 });
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

  it("bends a tall connection so its arrow follows the approach", () => {
    const { canvas, workflow } = harness();
    workflow.edges.push({ id: "start-end", source: "start", target: "end" });
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
    const remove = document.querySelector<HTMLButtonElement>(".context-menu__item")!;
    expect(remove.textContent).toContain("Delete connection");
    remove.click();

    expect(workflow.edges).toHaveLength(0);
  });

  it("deletes a node from its context menu", () => {
    const { canvas, workflow } = harness();
    canvas.addNode(descriptor("core.Log"), 200, 100);
    canvas.render();

    const node = document.querySelector<SVGGElement>('[data-node-id="log"]')!;
    node.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 80, clientY: 80 }));
    const remove = document.querySelector<HTMLButtonElement>(".context-menu__item")!;
    expect(remove.textContent).toContain("Delete node");
    remove.click();

    expect(workflow.nodes.some((candidate) => candidate.id === "log")).toBe(false);
  });
});
