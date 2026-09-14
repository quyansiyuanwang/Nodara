/**
 * The graph editor.
 *
 * A deliberately small SVG editor: nodes are draggable, ports connect by
 * dragging from an output to an input, and selection drives the inspector. It
 * renders the *document*, never the runtime's execution state, so it stays a
 * pure function of the workflow plus a run-status overlay.
 */

import {
  defaultConfig,
  edgeExists,
  nextEdgeId,
  nextNodeId,
} from "../model/workflow";
import { NodeDescriptor, RunStatus, Workflow, WorkflowNode } from "../runtime/types";

const NODE_WIDTH = 168;
const NODE_HEIGHT = 56;

export interface CanvasHandlers {
  onChange: () => void;
  onSelect: (nodeId: string | null) => void;
  onStatus: (message: string) => void;
  /** Resolve a node type to its descriptor, for port rendering. */
  descriptorFor: (nodeType: string) => NodeDescriptor | undefined;
}

interface PendingConnection {
  sourceId: string;
  sourcePort: string;
  x: number;
  y: number;
}

export class Canvas {
  private readonly nodesLayer: SVGGElement;
  private readonly edgesLayer: SVGGElement;
  private readonly pendingEdge: SVGPathElement;
  private selected: string | null = null;
  private selectedEdge: string | null = null;
  private pending: PendingConnection | null = null;
  private drag: { nodeId: string; offsetX: number; offsetY: number } | null = null;
  private status: RunStatus | null = null;

  constructor(
    private readonly svg: SVGSVGElement,
    private readonly workflow: Workflow,
    private readonly handlers: CanvasHandlers,
  ) {
    this.nodesLayer = svg.querySelector("#nodes")!;
    this.edgesLayer = svg.querySelector("#edges")!;
    this.pendingEdge = svg.querySelector("#pending-edge")!;

    svg.addEventListener("dragover", (event) => event.preventDefault());
    svg.addEventListener("drop", (event) => this.onDrop(event));
    svg.addEventListener("pointermove", (event) => this.onPointerMove(event));
    svg.addEventListener("pointerup", () => this.endInteraction());
    svg.addEventListener("pointerdown", (event) => {
      if (event.target === svg) this.select(null);
    });
    window.addEventListener("keydown", (event) => this.onKeyDown(event));
  }

  /** Update the run-status overlay used to highlight nodes. */
  setStatus(status: RunStatus | null): void {
    this.status = status;
    this.svg.dataset.status = status ?? "";
  }

  /** Highlight a node as the one currently executing. */
  setActiveNode(nodeId: string | null): void {
    for (const element of this.nodesLayer.querySelectorAll<SVGGElement>(".node")) {
      element.classList.toggle("node--active", element.dataset.nodeId === nodeId);
    }
  }

  /** Mark a node as finished, failed or skipped. */
  setNodeState(nodeId: string, state: "running" | "done" | "failed"): void {
    for (const element of this.nodesLayer.querySelectorAll<SVGGElement>(".node")) {
      if (element.dataset.nodeId !== nodeId) continue;
      element.classList.remove("node--running", "node--done", "node--failed");
      element.classList.add(`node--${state}`);
    }
  }

  /** Clear all execution decoration. */
  clearStates(): void {
    for (const element of this.nodesLayer.querySelectorAll<SVGGElement>(".node")) {
      element.classList.remove("node--running", "node--done", "node--failed", "node--active");
    }
  }

  select(nodeId: string | null, edgeId: string | null = null): void {
    this.selected = nodeId;
    this.selectedEdge = edgeId;
    for (const element of this.nodesLayer.querySelectorAll<SVGGElement>(".node")) {
      element.classList.toggle("node--selected", element.dataset.nodeId === nodeId);
    }
    for (const element of this.edgesLayer.querySelectorAll<SVGPathElement>(".edge")) {
      element.classList.toggle("edge--selected", element.dataset.edgeId === edgeId);
    }
    this.handlers.onSelect(nodeId);
  }

  selectedNodeId(): string | null {
    return this.selected;
  }

  /** Re-render everything. Called after any document mutation. */
  render(): void {
    this.renderEdges();
    this.renderNodes();
  }

  /** Place a node, offsetting so two drops never land exactly on top. */
  private onDrop(event: DragEvent): void {
    event.preventDefault();
    const nodeType =
      event.dataTransfer?.getData("application/x-nodara-node-type") ||
      event.dataTransfer?.getData("text/plain");
    if (!nodeType) return;
    const descriptor = this.handlers.descriptorFor(nodeType);
    if (!descriptor) {
      this.handlers.onStatus(`unknown node type \`${nodeType}\``);
      return;
    }
    const point = this.toCanvas(event.clientX, event.clientY);
    this.addNode(descriptor, point.x - NODE_WIDTH / 2, point.y - NODE_HEIGHT / 2);
  }

  /** Add a node programmatically (used by double-click in the palette). */
  addNode(descriptor: NodeDescriptor, x: number, y: number): void {
    const id = nextNodeId(descriptor, this.workflow.nodes);
    const node: WorkflowNode = {
      id,
      type: descriptor.node_type,
      label: descriptor.display_name,
      // Schema defaults apply to a new node, exactly as they would when the
      // document is completed in a text editor.
      config: defaultConfig(descriptor),
      position: {
        x: Math.round(x + this.workflow.nodes.length * 12) % 1400,
        y: Math.round(y + this.workflow.nodes.length * 12) % 800,
      },
    };
    this.workflow.nodes.push(node);
    this.select(id);
    this.handlers.onChange();
  }

  private onPointerMove(event: PointerEvent): void {
    if (this.drag) {
      const point = this.toCanvas(event.clientX, event.clientY);
      const node = this.workflow.nodes.find((candidate) => candidate.id === this.drag!.nodeId);
      if (!node) return;
      node.position = {
        x: Math.max(0, Math.round(point.x - this.drag.offsetX)),
        y: Math.max(0, Math.round(point.y - this.drag.offsetY)),
      };
      this.render();
      return;
    }
    if (this.pending) {
      const point = this.toCanvas(event.clientX, event.clientY);
      this.pending.x = point.x;
      this.pending.y = point.y;
      this.drawPendingEdge();
    }
  }

  private endInteraction(): void {
    if (this.pending) {
      this.pending = null;
      this.pendingEdge.classList.add("is-hidden");
      this.pendingEdge.removeAttribute("d");
    }
    if (this.drag) {
      this.drag = null;
      this.handlers.onChange();
    }
  }

  private onKeyDown(event: KeyboardEvent): void {
    const target = event.target as HTMLElement | null;
    if (target && ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName)) return;
    if (event.key !== "Delete" && event.key !== "Backspace") return;

    if (this.selectedEdge) {
      this.workflow.edges = this.workflow.edges.filter((edge) => edge.id !== this.selectedEdge);
      this.selectedEdge = null;
      this.handlers.onChange();
      return;
    }
    if (this.selected) {
      const id = this.selected;
      this.workflow.nodes = this.workflow.nodes.filter((node) => node.id !== id);
      this.workflow.edges = this.workflow.edges.filter(
        (edge) => edge.source !== id && edge.target !== id,
      );
      this.select(null);
      this.handlers.onChange();
    }
  }

  private renderNodes(): void {
    this.nodesLayer.replaceChildren();
    for (const node of this.workflow.nodes) {
      const descriptor = this.handlers.descriptorFor(node.type);
      const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
      group.classList.add("node");
      group.dataset.nodeId = node.id;
      if (this.selected === node.id) group.classList.add("node--selected");
      if (descriptor?.dangerous) group.classList.add("node--gated");
      if (this.status === "running") group.classList.add("node--ready");

      const x = node.position?.x ?? 0;
      const y = node.position?.y ?? 0;
      group.setAttribute("transform", `translate(${x}, ${y})`);

      const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
      rect.setAttribute("width", String(NODE_WIDTH));
      rect.setAttribute("height", String(NODE_HEIGHT));
      rect.setAttribute("rx", "8");
      rect.classList.add("node__body");
      group.appendChild(rect);

      const label = document.createElementNS("http://www.w3.org/2000/svg", "text");
      label.setAttribute("x", "12");
      label.setAttribute("y", "24");
      label.classList.add("node__label");
      label.textContent = node.label ?? descriptor?.display_name ?? node.type;
      group.appendChild(label);

      const type = document.createElementNS("http://www.w3.org/2000/svg", "text");
      type.setAttribute("x", "12");
      type.setAttribute("y", "42");
      type.classList.add("node__type");
      type.textContent = node.type;
      group.appendChild(type);

      const inputs = descriptor?.inputs ?? [];
      const outputs = descriptor?.outputs ?? [];
      inputs.forEach((port, index) => {
        group.appendChild(
          this.makePort(node.id, port.name, "input", index, inputs.length, 0),
        );
      });
      outputs.forEach((port, index) => {
        group.appendChild(
          this.makePort(node.id, port.name, "output", index, outputs.length, NODE_WIDTH),
        );
      });

      group.addEventListener("pointerdown", (event) => {
        if ((event.target as Element).classList.contains("port")) return;
        event.stopPropagation();
        const point = this.toCanvas(event.clientX, event.clientY);
        this.drag = {
          nodeId: node.id,
          offsetX: point.x - x,
          offsetY: point.y - y,
        };
        (event.target as Element).setPointerCapture?.(event.pointerId);
        this.select(node.id);
      });

      this.nodesLayer.appendChild(group);
    }
  }

  private makePort(
    nodeId: string,
    portName: string,
    kind: "input" | "output",
    index: number,
    total: number,
    xOffset: number,
  ): SVGCircleElement {
    const circle = document.createElementNS("http://www.w3.org/2000/svg", "circle");
    const step = NODE_HEIGHT / (total + 1);
    circle.setAttribute("cx", String(xOffset));
    circle.setAttribute("cy", String(step * (index + 1)));
    circle.setAttribute("r", "6");
    circle.classList.add("port", `port--${kind}`);
    circle.dataset.nodeId = nodeId;
    circle.dataset.port = portName;
    circle.dataset.kind = kind;

    if (kind === "output") {
      circle.addEventListener("pointerdown", (event) => {
        event.stopPropagation();
        const point = this.toCanvas(event.clientX, event.clientY);
        this.pending = { sourceId: nodeId, sourcePort: portName, x: point.x, y: point.y };
        this.pendingEdge.classList.remove("is-hidden");
        this.drawPendingEdge();
      });
    } else {
      circle.addEventListener("pointerup", (event) => {
        if (!this.pending) return;
        event.stopPropagation();
        this.connect(this.pending.sourceId, nodeId, this.pending.sourcePort, portName);
        this.pending = null;
        this.pendingEdge.classList.add("is-hidden");
      });
    }
    return circle;
  }

  private connect(source: string, target: string, sourcePort: string, targetPort: string): void {
    if (source === target) {
      this.handlers.onStatus("a node cannot connect to itself");
      return;
    }
    if (edgeExists(this.workflow.edges, source, target, sourcePort, targetPort)) {
      this.handlers.onStatus("that connection already exists");
      return;
    }
    this.workflow.edges.push({
      id: nextEdgeId(source, target, this.workflow.edges),
      source,
      target,
      source_port: sourcePort === "out" ? undefined : sourcePort,
      target_port: targetPort === "in" ? undefined : targetPort,
    });
    this.handlers.onChange();
  }

  private renderEdges(): void {
    this.edgesLayer.replaceChildren();
    for (const edge of this.workflow.edges) {
      const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
      path.classList.add("edge");
      path.dataset.edgeId = edge.id;
      if (edge.condition) path.classList.add("edge--guarded");
      if (this.selectedEdge === edge.id) path.classList.add("edge--selected");
      path.setAttribute("d", this.edgePath(edge.source, edge.target));
      path.addEventListener("pointerdown", (event) => {
        event.stopPropagation();
        this.select(null, edge.id);
      });
      path.appendChild(document.createElementNS("http://www.w3.org/2000/svg", "title"));
      path.lastChild!.textContent = edge.condition
        ? `${edge.source} → ${edge.target} when ${edge.condition}`
        : `${edge.source} → ${edge.target}`;
      this.edgesLayer.appendChild(path);
    }
  }

  private edgePath(sourceId: string, targetId: string): string {
    const source = this.nodeCenter(sourceId, "output");
    const target = this.nodeCenter(targetId, "input");
    const dx = Math.max(40, Math.abs(target.x - source.x) * 0.5);
    return `M ${source.x} ${source.y} C ${source.x + dx} ${source.y}, ${target.x - dx} ${target.y}, ${target.x} ${target.y}`;
  }

  private nodeCenter(nodeId: string, side: "input" | "output"): { x: number; y: number } {
    const node = this.workflow.nodes.find((candidate) => candidate.id === nodeId);
    const x = node?.position?.x ?? 0;
    const y = node?.position?.y ?? 0;
    return {
      x: side === "output" ? x + NODE_WIDTH : x,
      y: y + NODE_HEIGHT / 2,
    };
  }

  private drawPendingEdge(): void {
    if (!this.pending) return;
    const source = this.nodeCenter(this.pending.sourceId, "output");
    this.pendingEdge.setAttribute(
      "d",
      `M ${source.x} ${source.y} C ${source.x + 60} ${source.y}, ${this.pending.x - 60} ${this.pending.y}, ${this.pending.x} ${this.pending.y}`,
    );
  }

  private toCanvas(clientX: number, clientY: number): { x: number; y: number } {
    const rect = this.svg.getBoundingClientRect();
    return { x: clientX - rect.left, y: clientY - rect.top };
  }
}
