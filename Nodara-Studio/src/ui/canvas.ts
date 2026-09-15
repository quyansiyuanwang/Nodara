/**
 * The graph editor.
 *
 * A deliberately small SVG editor: nodes are draggable, ports connect by
 * dragging from an output to an input, and selection drives the inspector. It
 * renders the *document*, never the runtime's execution state, so it stays a
 * pure function of the workflow plus a run-status overlay.
 */

import { localizeProblem, t } from "../i18n";
import {
  defaultConfig,
  edgeExists,
  nextEdgeId,
  nextNodeId,
  nodeTypeAdmission,
  valueTypesCompatible,
} from "../model/workflow";
import { NodeDescriptor, RunStatus, Workflow, WorkflowNode } from "../runtime/types";

const NODE_WIDTH = 220;
const NODE_HEIGHT = 130;
const EDGE_MIN_HANDLE = 52;
const EDGE_MAX_HANDLE = 180;
const EDGE_PALETTE_DRAG_THRESHOLD = 5;

export interface CanvasHandlers {
  onChange: () => void;
  onSelect: (nodeId: string | null) => void;
  onStatus: (message: string) => void;
  /** Resolve a node type to its descriptor, for port rendering. */
  descriptorFor: (nodeType: string) => NodeDescriptor | undefined;
  /** Notify the shell when the view transform changes. */
  onViewChange?: (scale: number) => void;
}

interface PendingConnection {
  sourceId: string;
  sourcePort: string;
  edgeKind: "control" | "data";
  x: number;
  y: number;
  originX: number;
  originY: number;
  mode: "drag" | "click";
}

interface NodeDrag {
  primaryId: string;
  startX: number;
  startY: number;
  origins: Map<string, { x: number; y: number }>;
  moved: boolean;
}

interface MarqueeDrag {
  startX: number;
  startY: number;
  currentX: number;
  currentY: number;
}

interface EdgeAnimation {
  frame: number;
  runner: SVGPolygonElement;
  startedAt: number;
}

type ContextTarget =
  | { kind: "node"; id: string }
  | { kind: "edge"; id: string };

export class Canvas {
  private readonly viewport: SVGGElement;
  private readonly nodesLayer: SVGGElement;
  private readonly edgesLayer: SVGGElement;
  private readonly pendingEdge: SVGPathElement;
  private readonly selectionBox: SVGRectElement;
  private readonly quickConfig: HTMLDivElement;
  private readonly edgeAnimations = new Map<string, EdgeAnimation>();
  private selected = new Set<string>();
  private primarySelected: string | null = null;
  private selectedEdge: string | null = null;
  private readonly edgeStates = new Map<string, "active" | "data">();
  private pending: PendingConnection | null = null;
  private pendingDragMoved = false;
  private drag: NodeDrag | null = null;
  private marquee: MarqueeDrag | null = null;
  private paletteDragCleanup: (() => void) | null = null;
  private status: RunStatus | null = null;
  private readonly contextMenu: HTMLDivElement;
  private viewScale = 1;
  private viewX = 0;
  private viewY = 0;
  private pan: { clientX: number; clientY: number; viewX: number; viewY: number } | null = null;
  private spaceDown = false;
  private readonly cleanupCallbacks: Array<() => void> = [];

  constructor(
    private readonly svg: SVGSVGElement,
    private readonly workflow: Workflow,
    private readonly handlers: CanvasHandlers,
  ) {
    this.viewport = svg.querySelector("#viewport")!;
    this.nodesLayer = svg.querySelector("#nodes")!;
    this.edgesLayer = svg.querySelector("#edges")!;
    this.pendingEdge = svg.querySelector("#pending-edge")!;
    this.selectionBox = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    this.selectionBox.classList.add("canvas-selection-box", "is-hidden");
    this.viewport.appendChild(this.selectionBox);
    this.quickConfig = document.createElement("div");
    this.quickConfig.className = "node-quick-config is-hidden";
    (svg.closest<HTMLElement>(".canvas-wrap") ?? svg.parentElement ?? document.body).appendChild(this.quickConfig);

    this.contextMenu = document.createElement("div");
    this.contextMenu.className = "context-menu";
    this.contextMenu.hidden = true;
    document.body.appendChild(this.contextMenu);

    const closeContextMenu = (event: PointerEvent) => {
      if (!this.contextMenu.hidden && !this.contextMenu.contains(event.target as Node)) {
        this.contextMenu.hidden = true;
      }
    };
    const blurContextMenu = () => {
      this.contextMenu.hidden = true;
    };
    const pointerMove = (event: PointerEvent) => this.onPointerMove(event);
    const pointerUp = () => this.endInteraction();
    const svgPointerDown = (event: PointerEvent) => {
      const target = event.target as Element;
      if (target.closest(".node, .edge-hit, .port, .exec-port")) return;
      if (event.button === 1 || this.spaceDown) {
        event.preventDefault();
        this.pan = {
          clientX: event.clientX,
          clientY: event.clientY,
          viewX: this.viewX,
          viewY: this.viewY,
        };
        svg.classList.add("canvas--panning");
        return;
      }
      if (event.button !== 0) return;
      event.preventDefault();
      const point = this.toCanvas(event.clientX, event.clientY);
      this.setSelection([], null);
      this.marquee = { startX: point.x, startY: point.y, currentX: point.x, currentY: point.y };
      this.selectionBox.classList.remove("is-hidden");
      this.drawSelectionBox();
    };
    const wheel = (event: WheelEvent) => {
      event.preventDefault();
      const rect = svg.getBoundingClientRect();
      this.zoomBy(Math.exp(-event.deltaY * 0.001), {
        x: event.clientX - rect.left,
        y: event.clientY - rect.top,
      });
    };
    const keyDown = (event: KeyboardEvent) => this.onKeyDown(event);
    const keyUp = (event: KeyboardEvent) => {
      if (event.key === " ") this.spaceDown = false;
    };

    document.addEventListener("pointerdown", closeContextMenu);
    window.addEventListener("blur", blurContextMenu);
    window.addEventListener("pointermove", pointerMove);
    window.addEventListener("pointerup", pointerUp);
    window.addEventListener("pointercancel", pointerUp);
    svg.addEventListener("pointerdown", svgPointerDown);
    svg.addEventListener("wheel", wheel, { passive: false });
    window.addEventListener("keydown", keyDown);
    window.addEventListener("keyup", keyUp);
    this.cleanupCallbacks.push(
      () => document.removeEventListener("pointerdown", closeContextMenu),
      () => window.removeEventListener("blur", blurContextMenu),
      () => window.removeEventListener("pointermove", pointerMove),
      () => window.removeEventListener("pointerup", pointerUp),
      () => window.removeEventListener("pointercancel", pointerUp),
      () => svg.removeEventListener("pointerdown", svgPointerDown),
      () => svg.removeEventListener("wheel", wheel),
      () => window.removeEventListener("keydown", keyDown),
      () => window.removeEventListener("keyup", keyUp),
    );
    this.applyView();  }

  /** Remove global listeners and transient overlays. */
  destroy(): void {
    this.paletteDragCleanup?.();
    this.cancelAllEdgeAnimations();
    for (const cleanup of this.cleanupCallbacks.splice(0)) cleanup();
    this.contextMenu.remove();
    this.quickConfig.remove();
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

  /** Mark an execution or data edge as traversed during the current run. */
  setEdgeState(edgeId: string, state: "active" | "data"): void {
    this.edgeStates.set(edgeId, state);
    const group = this.edgesLayer.querySelector<SVGGElement>(
      `.edge-group[data-edge-id="${this.escapeId(edgeId)}"]`,
    );
    if (!group) return;
    const path = group.querySelector<SVGPathElement>(".edge");
    if (!path) return;
    path.classList.toggle("edge--active", state === "active");
    path.classList.toggle("edge--data-active", state === "data");
    this.ensureEdgeAnimation(edgeId, group, path, state);
  }

  /** Clear all execution decoration. */
  clearStates(): void {
    this.cancelAllEdgeAnimations();
    this.edgeStates.clear();
    for (const element of this.nodesLayer.querySelectorAll<SVGGElement>(".node")) {
      element.classList.remove("node--running", "node--done", "node--failed", "node--active");
    }
    for (const group of this.edgesLayer.querySelectorAll<SVGGElement>(".edge-group")) {
      group.querySelector(".edge-flow")?.remove();
      group.querySelector(".edge-runner")?.remove();
      group.querySelector(".edge")?.classList.remove("edge--active", "edge--data-active");
    }
  }

  /**
   * Animate a small arrow along an activated edge with requestAnimationFrame
   * rather than SVG SMIL. SMIL is inconsistently enabled in desktop WebViews,
   * which made the previous `animateMotion` pulse appear to do nothing even
   * though the edge state was correct.
   */
  private ensureEdgeAnimation(
    edgeId: string,
    group: SVGGElement,
    path: SVGPathElement,
    state: "active" | "data",
  ): void {
    this.cancelEdgeAnimation(edgeId);

    const pathData = path.getAttribute("d") ?? "";
    let flow = group.querySelector<SVGPathElement>(".edge-flow");
    if (!flow) {
      flow = document.createElementNS("http://www.w3.org/2000/svg", "path");
      flow.classList.add("edge-flow");
      group.appendChild(flow);
    }
    flow.classList.toggle("edge-flow--data", state === "data");
    flow.setAttribute("d", pathData);

    let runner = group.querySelector<SVGPolygonElement>(".edge-runner");
    if (!runner) {
      runner = document.createElementNS("http://www.w3.org/2000/svg", "polygon");
      runner.classList.add("edge-runner");
      runner.setAttribute("points", "-6,-4 8,0 -6,4");
      group.appendChild(runner);
    }
    runner.classList.toggle("edge-runner--data", state === "data");
    runner.setAttribute("transform", "translate(0 0)");

    // jsdom (used by unit tests) does not implement SVG path length sampling.
    if (
      typeof requestAnimationFrame !== "function" ||
      typeof path.getTotalLength !== "function" ||
      typeof path.getPointAtLength !== "function"
    ) {
      return;
    }

    const animation: EdgeAnimation = { frame: 0, runner, startedAt: 0 };
    const tick = (timestamp: number) => {
      if (this.edgeAnimations.get(edgeId) !== animation) return;
      if (animation.startedAt === 0) animation.startedAt = timestamp;

      const length = path.getTotalLength();
      if (length > 0) {
        const duration = Math.max(900, Math.min(2400, length * 5));
        const elapsed = (timestamp - animation.startedAt) % duration;
        const distance = (elapsed / duration) * length;
        const point = path.getPointAtLength(distance);
        const ahead = path.getPointAtLength(Math.min(length, distance + 2));
        const angle = Math.atan2(ahead.y - point.y, ahead.x - point.x) * (180 / Math.PI);
        runner.setAttribute("transform", `translate(${point.x} ${point.y}) rotate(${angle})`);
      }

      animation.frame = requestAnimationFrame(tick);
    };
    this.edgeAnimations.set(edgeId, animation);
    animation.frame = requestAnimationFrame(tick);
  }

  private cancelEdgeAnimation(edgeId: string): void {
    const animation = this.edgeAnimations.get(edgeId);
    if (!animation) return;
    cancelAnimationFrame(animation.frame);
    this.edgeAnimations.delete(edgeId);
  }

  private cancelAllEdgeAnimations(): void {
    for (const animation of this.edgeAnimations.values()) cancelAnimationFrame(animation.frame);
    this.edgeAnimations.clear();
  }

  select(nodeId: string | null, edgeId: string | null = null): void {
    this.setSelection(nodeId ? [nodeId] : [], nodeId);
    this.selectedEdge = edgeId;
    this.updateSelectionVisuals();
  }

  selectedNodeIds(): string[] {
    return [...this.selected];
  }

  private setSelection(ids: Iterable<string>, primary: string | null): void {
    this.selected = new Set(ids);
    this.primarySelected = primary && this.selected.has(primary)
      ? primary
      : this.selected.values().next().value ?? null;
    this.selectedEdge = null;
    this.updateSelectionVisuals();
  }

  private updateSelectionVisuals(): void {
    for (const element of this.nodesLayer.querySelectorAll<SVGGElement>(".node")) {
      element.classList.toggle(
        "node--selected",
        this.selected.has(element.dataset.nodeId ?? ""),
      );
    }
    for (const element of this.edgesLayer.querySelectorAll<SVGPathElement>(".edge")) {
      element.classList.toggle("edge--selected", element.dataset.edgeId === this.selectedEdge);
    }
    this.renderQuickConfig();
    this.handlers.onSelect(this.primarySelected);
  }

  private toggleSelection(nodeId: string): boolean {
    if (this.selected.has(nodeId)) {
      this.selected.delete(nodeId);
      if (this.primarySelected === nodeId) {
        this.primarySelected = this.selected.values().next().value ?? null;
      }
      this.updateSelectionVisuals();
      return false;
    }
    this.selected.add(nodeId);
    this.primarySelected = nodeId;
    this.updateSelectionVisuals();
    return true;
  }

  private selectPathTo(targetId: string): void {
    const anchor = this.primarySelected;
    if (!anchor || anchor === targetId) {
      this.setSelection([targetId], targetId);
      return;
    }
    const forward = this.reachable(anchor, "forward");
    const reverse = this.reachable(targetId, "reverse");
    const ids = [...forward].filter((id) => reverse.has(id));
    if (!ids.includes(anchor)) ids.push(anchor);
    if (!ids.includes(targetId)) ids.push(targetId);
    if (!forward.has(targetId)) {
      this.handlers.onStatus(t("canvas.noPath", { source: anchor, target: targetId }));
      return;
    }
    this.setSelection(ids, targetId);
  }

  private reachable(startId: string, direction: "forward" | "reverse"): Set<string> {
    const result = new Set([startId]);
    const queue = [startId];
    while (queue.length > 0) {
      const current = queue.shift()!;
      for (const edge of this.workflow.edges) {
        if (edge.kind !== "control") continue;
        const next = direction === "forward" ? edge.source === current && edge.target : edge.target === current && edge.source;
        if (!next || typeof next !== "string" || result.has(next)) continue;
        result.add(next);
        queue.push(next);
      }
    }
    return result;
  }

  /** Select a node or edge and centre it in the visible canvas. */
  focus(nodeId: string | null, edgeId: string | null = null): void {
    this.select(nodeId, edgeId);
    let center: { x: number; y: number } | null = null;
    if (nodeId) {
      const node = this.workflow.nodes.find((candidate) => candidate.id === nodeId);
      if (node) {
        center = {
          x: (node.position?.x ?? 0) + NODE_WIDTH / 2,
          y: (node.position?.y ?? 0) + NODE_HEIGHT / 2,
        };
      }
    } else if (edgeId) {
      const edge = this.workflow.edges.find((candidate) => candidate.id === edgeId);
      if (edge) {
        const source = this.portCenter(edge.source, "data", "output", edge.source_port);
        const target = this.portCenter(edge.target, "data", "input", edge.target_port);
        center = { x: (source.x + target.x) / 2, y: (source.y + target.y) / 2 };
      }
    }
    if (!center) return;
    const rect = this.svg.getBoundingClientRect();
    this.viewX = rect.width / 2 - center.x * this.viewScale;
    this.viewY = rect.height / 2 - center.y * this.viewScale;
    this.applyView();
  }

  selectedNodeId(): string | null {
    return this.primarySelected;
  }

  selectedEdgeId(): string | null {
    return this.selectedEdge;
  }

  /** Current canvas scale, useful to the shell and tests. */
  currentScale(): number {
    return this.viewScale;
  }

  /** Arrange nodes in deterministic left-to-right topology levels. */
  autoLayout(): void {
    if (this.workflow.nodes.length === 0) return;
    const known = new Set(this.workflow.nodes.map((node) => node.id));
    const outgoing = new Map<string, string[]>();
    const indegree = new Map<string, number>();
    for (const node of this.workflow.nodes) {
      outgoing.set(node.id, []);
      indegree.set(node.id, 0);
    }
    for (const edge of this.workflow.edges) {
      if (edge.kind !== "control" || !known.has(edge.source) || !known.has(edge.target)) continue;
      outgoing.get(edge.source)!.push(edge.target);
      indegree.set(edge.target, (indegree.get(edge.target) ?? 0) + 1);
    }

    const nodeOrder = new Map(this.workflow.nodes.map((node, index) => [node.id, index]));
    const ready = this.workflow.nodes
      .filter((node) => indegree.get(node.id) === 0)
      .map((node) => node.id);
    const levels = new Map<string, number>();
    for (const id of ready) levels.set(id, 0);

    while (ready.length > 0) {
      ready.sort((a, b) => (nodeOrder.get(a) ?? 0) - (nodeOrder.get(b) ?? 0));
      const id = ready.shift()!;
      const level = levels.get(id) ?? 0;
      for (const target of outgoing.get(id) ?? []) {
        levels.set(target, Math.max(levels.get(target) ?? 0, level + 1));
        const next = (indegree.get(target) ?? 0) - 1;
        indegree.set(target, next);
        if (next === 0) ready.push(target);
      }
    }

    let fallbackLevel = Math.max(0, ...levels.values()) + 1;
    for (const node of this.workflow.nodes) {
      if (!levels.has(node.id)) levels.set(node.id, fallbackLevel++);
    }

    const groups = new Map<number, string[]>();
    for (const [id, level] of levels) {
      const group = groups.get(level) ?? [];
      group.push(id);
      groups.set(level, group);
    }
    const positions = new Map(this.workflow.nodes.map((node) => [node.id, node.position]));
    for (const [level, ids] of [...groups.entries()].sort(([a], [b]) => a - b)) {
      ids.sort((a, b) => {
        const left = positions.get(a);
        const right = positions.get(b);
        return (left?.y ?? 0) - (right?.y ?? 0) || (nodeOrder.get(a) ?? 0) - (nodeOrder.get(b) ?? 0);
      });
      ids.forEach((id, row) => {
        const node = this.workflow.nodes.find((candidate) => candidate.id === id);
        if (!node) return;
        node.position = {
          x: 80 + level * (NODE_WIDTH + 90),
          y: 80 + row * (NODE_HEIGHT + 50),
        };
      });
    }
    this.handlers.onChange();
  }

  zoomIn(): void {
    this.zoomBy(1.2);
  }

  zoomOut(): void {
    this.zoomBy(1 / 1.2);
  }

  resetView(): void {
    this.viewScale = 1;
    this.viewX = 0;
    this.viewY = 0;
    this.applyView();
  }

  fitToContent(): void {
    if (this.workflow.nodes.length === 0) {
      this.resetView();
      return;
    }
    const rect = this.svg.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return;
    const xs = this.workflow.nodes.map((node) => node.position?.x ?? 0);
    const ys = this.workflow.nodes.map((node) => node.position?.y ?? 0);
    const minX = Math.min(...xs);
    const minY = Math.min(...ys);
    const maxX = Math.max(...xs.map((x) => x + NODE_WIDTH));
    const maxY = Math.max(...ys.map((y) => y + NODE_HEIGHT));
    const contentWidth = Math.max(1, maxX - minX);
    const contentHeight = Math.max(1, maxY - minY);
    this.viewScale = Math.max(
      0.2,
      Math.min(2, (rect.width - 80) / contentWidth, (rect.height - 80) / contentHeight),
    );
    this.viewX = rect.width / 2 - ((minX + maxX) / 2) * this.viewScale;
    this.viewY = rect.height / 2 - ((minY + maxY) / 2) * this.viewScale;
    this.applyView();
  }

  /** Re-render everything. Called after any document mutation. */
  render(): void {
    this.renderEdges();
    this.renderNodes();
  }

  /**
   * Begin a palette drag. Pointer events avoid WebView-specific failures with
   * native HTML5 drag-and-drop and give us a real drop position on the SVG.
   */
  beginPaletteDrag(descriptor: NodeDescriptor, event: PointerEvent): void {
    if (event.button !== 0) return;
    this.paletteDragCleanup?.();

    const source = event.currentTarget instanceof HTMLElement ? event.currentTarget : null;
    const canvasWrap = this.svg.closest<HTMLElement>(".canvas-wrap") ?? this.svg.parentElement;
    const ghost = document.createElement("div");
    ghost.className = "palette-drag-ghost";
    ghost.textContent = descriptor.display_name;
    ghost.hidden = true;
    document.body.appendChild(ghost);

    const startX = event.clientX;
    const startY = event.clientY;
    let active = false;
    let overCanvas = false;

    const isOverCanvas = (pointerEvent: PointerEvent): boolean => {
      const rect = this.svg.getBoundingClientRect();
      return (
        rect.width > 0 &&
        rect.height > 0 &&
        pointerEvent.clientX >= rect.left &&
        pointerEvent.clientX <= rect.right &&
        pointerEvent.clientY >= rect.top &&
        pointerEvent.clientY <= rect.bottom
      );
    };

    const cleanup = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onCancel);
      ghost.remove();
      source?.classList.remove("palette__item--dragging");
      document.body.classList.remove("is-palette-dragging");
      canvasWrap?.classList.remove("canvas-wrap--drop-target");
      this.paletteDragCleanup = null;
    };

    const onMove = (moveEvent: PointerEvent) => {
      if (
        !active &&
        Math.hypot(moveEvent.clientX - startX, moveEvent.clientY - startY) <
          EDGE_PALETTE_DRAG_THRESHOLD
      ) {
        return;
      }
      if (!active) {
        active = true;
        ghost.hidden = false;
        source?.classList.add("palette__item--dragging");
        document.body.classList.add("is-palette-dragging");
      }
      moveEvent.preventDefault();
      ghost.style.left = `${moveEvent.clientX + 14}px`;
      ghost.style.top = `${moveEvent.clientY + 14}px`;
      overCanvas = isOverCanvas(moveEvent);
      canvasWrap?.classList.toggle("canvas-wrap--drop-target", overCanvas);
    };

    const onUp = (upEvent: PointerEvent) => {
      const dropped = active && isOverCanvas(upEvent);
      cleanup();
      if (!dropped) return;
      const point = this.toCanvas(upEvent.clientX, upEvent.clientY);
      this.addNode(descriptor, point.x - NODE_WIDTH / 2, point.y - NODE_HEIGHT / 2, false);
    };

    const onCancel = () => cleanup();

    this.paletteDragCleanup = cleanup;
    window.addEventListener("pointermove", onMove, { passive: false });
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onCancel);
  }

  /** Add a node near the centre of the visible canvas (used by palette clicks). */
  addNodeAtViewportCenter(descriptor: NodeDescriptor): void {
    const rect = this.svg.getBoundingClientRect();
    const center = this.toCanvas(rect.left + rect.width / 2, rect.top + rect.height / 2);
    const index = this.workflow.nodes.length;
    const jitter = ((index % 5) - 2) * 18;
    const stagger = Math.min(index, 6) * 12;
    const x = center.x - NODE_WIDTH / 2 + jitter - stagger;
    const y = center.y - NODE_HEIGHT / 2 + jitter - stagger;
    this.addNode(descriptor, x, y);
  }

  /** Add a node programmatically (used by palette clicks and exact drops). */
  addNode(descriptor: NodeDescriptor, x: number, y: number, stagger = true): void {
    const admission = nodeTypeAdmission(this.workflow, descriptor.node_type);
    if (!admission.allowed) {
      this.handlers.onStatus(localizeProblem(admission.reason ?? ""));
      return;
    }

    const id = nextNodeId(descriptor, this.workflow.nodes);
    const stackOffset = stagger ? this.workflow.nodes.length * 12 : 0;
    const node: WorkflowNode = {
      id,
      type: descriptor.node_type,
      label: descriptor.display_name,
      // Schema defaults apply to a new node, exactly as they would when the
      // document is completed in a text editor.
      config: defaultConfig(descriptor),
      position: {
        x: Math.round(x + stackOffset),
        y: Math.round(y + stackOffset),
      },
    };
    this.workflow.nodes.push(node);
    this.select(id);
    this.handlers.onChange();
  }

  private onPointerMove(event: PointerEvent): void {
    if (this.pan) {
      this.viewX = this.pan.viewX + event.clientX - this.pan.clientX;
      this.viewY = this.pan.viewY + event.clientY - this.pan.clientY;
      this.applyView();
      return;
    }
    if (this.drag) {
      const point = this.toCanvas(event.clientX, event.clientY);
      const dx = point.x - this.drag.startX;
      const dy = point.y - this.drag.startY;
      let moved = false;
      for (const [id, origin] of this.drag.origins) {
        const node = this.workflow.nodes.find((candidate) => candidate.id === id);
        if (!node) continue;
        const x = Math.round(origin.x + dx);
        const y = Math.round(origin.y + dy);
        if (node.position?.x === x && node.position?.y === y) continue;
        node.position = { x, y };
        const element = this.nodesLayer.querySelector<SVGGElement>(
          `.node[data-node-id="${this.escapeId(id)}"]`,
        );
        element?.setAttribute("transform", `translate(${x}, ${y})`);
        moved = true;
      }
      if (moved) {
        this.drag.moved = true;
        this.renderEdges();
        this.renderQuickConfig();
      }
      return;
    }
    if (this.marquee) {
      const point = this.toCanvas(event.clientX, event.clientY);
      this.marquee.currentX = point.x;
      this.marquee.currentY = point.y;
      this.drawSelectionBox();
      return;
    }
    if (this.pending) {
      const point = this.toCanvas(event.clientX, event.clientY);
      if (
        Math.hypot(point.x - this.pending.originX, point.y - this.pending.originY) >= 5
      ) {
        this.pendingDragMoved = true;
      }
      this.pending.x = point.x;
      this.pending.y = point.y;
      this.drawPendingEdge();
    }
  }

  private endInteraction(): void {
    if (this.pan) {
      this.pan = null;
      this.svg.classList.remove("canvas--panning");
    }
    if (this.pending) {
      this.pending = null;
      this.pendingEdge.classList.add("is-hidden");
      this.pendingEdge.removeAttribute("d");
    }
    if (this.drag) {
      const changed = this.drag.moved;
      this.drag = null;
      document.body.classList.remove("is-canvas-dragging");
      if (changed) this.handlers.onChange();
    }
    if (this.marquee) {
      const box = this.marquee;
      this.marquee = null;
      this.selectionBox.classList.add("is-hidden");
      const left = Math.min(box.startX, box.currentX);
      const right = Math.max(box.startX, box.currentX);
      const top = Math.min(box.startY, box.currentY);
      const bottom = Math.max(box.startY, box.currentY);
      const ids = this.workflow.nodes
        .filter((node) => {
          const x = node.position?.x ?? 0;
          const y = node.position?.y ?? 0;
          return x <= right && x + NODE_WIDTH >= left && y <= bottom && y + NODE_HEIGHT >= top;
        })
        .map((node) => node.id);
      this.setSelection(ids, ids[0] ?? null);
    }
  }

  private escapeId(value: string): string {
    return typeof CSS !== "undefined" && typeof CSS.escape === "function"
      ? CSS.escape(value)
      : value.replace(/[^A-Za-z0-9_-]/g, "\\$&");
  }

  private drawSelectionBox(): void {
    if (!this.marquee) return;
    const left = Math.min(this.marquee.startX, this.marquee.currentX);
    const top = Math.min(this.marquee.startY, this.marquee.currentY);
    this.selectionBox.setAttribute("x", String(left));
    this.selectionBox.setAttribute("y", String(top));
    this.selectionBox.setAttribute("width", String(Math.abs(this.marquee.currentX - this.marquee.startX)));
    this.selectionBox.setAttribute("height", String(Math.abs(this.marquee.currentY - this.marquee.startY)));
  }

  private onKeyDown(event: KeyboardEvent): void {
    const target = event.target as HTMLElement | null;
    if (target && ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName)) return;
    if (event.key === " ") {
      this.spaceDown = true;
      event.preventDefault();
      return;
    }
    if (event.key === "Escape" && this.pending) {
      event.preventDefault();
      this.pending = null;
      this.pendingEdge.classList.add("is-hidden");
      this.pendingEdge.removeAttribute("d");
      return;
    }
    if ((event.ctrlKey || event.metaKey) && event.key === "0") {
      event.preventDefault();
      this.resetView();
      return;
    }
    if ((event.ctrlKey || event.metaKey) && (event.key === "+" || event.key === "=")) {
      event.preventDefault();
      this.zoomIn();
      return;
    }
    if ((event.ctrlKey || event.metaKey) && event.key === "-") {
      event.preventDefault();
      this.zoomOut();
      return;
    }
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "d") {
      const primary = this.primarySelected;
      if (!primary) return;
      event.preventDefault();
      this.duplicateNode(primary);
      return;
    }
    if (event.key === "F9" && this.primarySelected) {
      event.preventDefault();
      this.toggleBreakpoint(this.primarySelected);
      return;
    }
    if (event.key !== "Delete" && event.key !== "Backspace") return;
    this.deleteSelection();
  }

  private toggleBreakpoint(id: string): void {
    const ids = this.selected.has(id) ? [...this.selected] : [id];
    const nodes = this.workflow.nodes.filter((node) => ids.includes(node.id));
    if (nodes.length === 0) return;
    const set = !nodes.every((node) => node.breakpoint);
    for (const node of nodes) {
      if (set) node.breakpoint = true;
      else delete node.breakpoint;
    }
    this.handlers.onChange();
  }

  private duplicateNode(id: string): void {
    const source = this.workflow.nodes.find((node) => node.id === id);
    if (!source) return;
    const admission = nodeTypeAdmission(this.workflow, source.type);
    if (!admission.allowed) {
      this.handlers.onStatus(localizeProblem(admission.reason ?? ""));
      return;
    }
    const descriptor = this.handlers.descriptorFor(source.type);
    if (!descriptor) {
      this.handlers.onStatus(`cannot duplicate unknown node type \`${source.type}\``);
      return;
    }
    const nextId = nextNodeId(descriptor, this.workflow.nodes);
    const position = {
      x: (source.position?.x ?? 0) + 28,
      y: (source.position?.y ?? 0) + 28,
    };
    const copy: WorkflowNode = {
      ...source,
      id: nextId,
      label: source.label ? `${source.label} copy` : descriptor.display_name,
      config: structuredClone(source.config ?? {}),
      position,
    };
    this.workflow.nodes.push(copy);
    this.select(nextId);
    this.handlers.onChange();
  }

  private deleteSelection(): void {
    this.contextMenu.hidden = true;
    if (this.selectedEdge) {
      this.workflow.edges = this.workflow.edges.filter((edge) => edge.id !== this.selectedEdge);
      this.select(null);
      this.handlers.onChange();
      return;
    }
    if (this.selected.size > 0) {
      const ids = new Set(this.selected);
      this.workflow.nodes = this.workflow.nodes.filter((node) => !ids.has(node.id));
      this.workflow.edges = this.workflow.edges.filter(
        (edge) => !ids.has(edge.source) && !ids.has(edge.target),
      );
      this.select(null);
      this.handlers.onChange();
    }
  }

  private showContextMenu(event: MouseEvent, target: ContextTarget): void {
    event.preventDefault();
    event.stopPropagation();
    if (target.kind === "node") {
      if (!this.selected.has(target.id)) this.select(target.id);
    } else {
      this.select(null, target.id);
    }

    this.contextMenu.replaceChildren();
    const title = document.createElement("div");
    title.className = "context-menu__title";
    title.textContent = target.kind === "node"
      ? t("canvas.nodeTitle", { id: target.id })
      : t("canvas.connectionTitle", { id: target.id });
    this.contextMenu.appendChild(title);

    if (target.kind === "node") {
      const node = this.workflow.nodes.find((candidate) => candidate.id === target.id);
      if (node) {
        const duplicate = document.createElement("button");
        duplicate.type = "button";
        duplicate.className = "context-menu__item";
        duplicate.dataset.action = "duplicate";
        const admission = nodeTypeAdmission(this.workflow, node.type);
        duplicate.disabled = !admission.allowed;
        duplicate.title = admission.allowed
          ? ""
          : localizeProblem(admission.reason ?? "");
        const duplicateLabel = document.createElement("span");
        duplicateLabel.textContent = t("canvas.duplicateNode");
        const duplicateShortcut = document.createElement("span");
        duplicateShortcut.className = "context-menu__shortcut";
        duplicateShortcut.textContent = "Ctrl+D";
        duplicate.append(duplicateLabel, duplicateShortcut);
        duplicate.addEventListener("click", () => {
          this.contextMenu.hidden = true;
          this.duplicateNode(target.id);
        });
        this.contextMenu.appendChild(duplicate);

        const breakpoint = document.createElement("button");
        breakpoint.type = "button";
        breakpoint.className = "context-menu__item";
        breakpoint.dataset.action = "breakpoint";
        const breakpointLabel = document.createElement("span");
        breakpointLabel.textContent = node.breakpoint
          ? t("canvas.clearBreakpoint")
          : t("canvas.setBreakpoint");
        const breakpointShortcut = document.createElement("span");
        breakpointShortcut.className = "context-menu__shortcut";
        breakpointShortcut.textContent = "F9";
        breakpoint.append(breakpointLabel, breakpointShortcut);
        breakpoint.addEventListener("click", () => {
          this.contextMenu.hidden = true;
          this.toggleBreakpoint(node.id);
        });
        this.contextMenu.appendChild(breakpoint);

        const toggle = document.createElement("button");
        toggle.type = "button";
        toggle.className = "context-menu__item";
        toggle.dataset.action = "toggle";
        const toggleLabel = document.createElement("span");
        toggleLabel.textContent = node.enabled === false
          ? t("canvas.enableNode")
          : t("canvas.disableNode");
        toggle.appendChild(toggleLabel);
        toggle.addEventListener("click", () => {
          const ids = this.selected.has(node.id) ? [...this.selected] : [node.id];
          const nodes = this.workflow.nodes.filter((candidate) => ids.includes(candidate.id));
          const enable = nodes.every((candidate) => candidate.enabled === false);
          for (const candidate of nodes) {
            if (enable) delete candidate.enabled;
            else candidate.enabled = false;
          }
          this.contextMenu.hidden = true;
          this.handlers.onChange();
        });
        this.contextMenu.appendChild(toggle);
      }
    }

    if (target.kind === "edge") {
      const edge = this.workflow.edges.find((candidate) => candidate.id === target.id);
      if (edge?.kind === "control") {
        for (const [value, key] of [
          ["always", "inspector.edgeBranchAlways"],
          ["success", "inspector.edgeBranchSuccess"],
          ["failure", "inspector.edgeBranchFailure"],
        ] as const) {
          const option = document.createElement("button");
          option.type = "button";
          option.className = "context-menu__item";
          option.dataset.action = `branch-${value}`;
          const label = document.createElement("span");
          label.textContent = t(key);
          const current = document.createElement("span");
          current.className = "context-menu__shortcut";
          current.textContent = (edge.branch ?? "always") === value ? "✓" : "";
          option.append(label, current);
          option.addEventListener("click", () => {
            if (value === "always") delete edge.branch;
            else edge.branch = value;
            this.contextMenu.hidden = true;
            this.handlers.onChange();
          });
          this.contextMenu.appendChild(option);
        }
      }
    }

    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "context-menu__item context-menu__item--danger";
    remove.dataset.action = "delete";
    const label = document.createElement("span");
    label.textContent = target.kind === "node"
      ? t("canvas.deleteNode")
      : t("canvas.deleteConnection");
    const shortcut = document.createElement("span");
    shortcut.className = "context-menu__shortcut";
    shortcut.textContent = "Del";
    remove.append(label, shortcut);
    remove.addEventListener("click", () => this.deleteSelection());
    this.contextMenu.appendChild(remove);

    this.contextMenu.hidden = false;
    const bounds = this.contextMenu.getBoundingClientRect();
    const left = Math.max(8, Math.min(event.clientX, window.innerWidth - bounds.width - 8));
    const top = Math.max(8, Math.min(event.clientY, window.innerHeight - bounds.height - 8));
    this.contextMenu.style.left = `${left}px`;
    this.contextMenu.style.top = `${top}px`;
  }

  private renderNodes(): void {
    this.nodesLayer.replaceChildren();
    for (const node of this.workflow.nodes) {
      const descriptor = this.handlers.descriptorFor(node.type);
      const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
      group.classList.add("node");
      group.dataset.nodeId = node.id;
      if (this.selected.has(node.id)) group.classList.add("node--selected");
      if (descriptor?.dangerous) group.classList.add("node--gated");
      if (node.enabled === false) group.classList.add("node--disabled");
      if (node.breakpoint) group.classList.add("node--breakpoint");
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

      if (node.breakpoint) {
        const marker = document.createElementNS("http://www.w3.org/2000/svg", "circle");
        marker.setAttribute("cx", "7");
        marker.setAttribute("cy", "7");
        marker.setAttribute("r", "4");
        marker.classList.add("node__breakpoint");
        const title = document.createElementNS("http://www.w3.org/2000/svg", "title");
        title.textContent = t("canvas.nodeBreakpoint");
        marker.appendChild(title);
        group.appendChild(marker);
      }

      if (node.enabled === false) {
        const disabled = document.createElementNS("http://www.w3.org/2000/svg", "text");
        disabled.setAttribute("x", String(NODE_WIDTH - 10));
        disabled.setAttribute("y", "16");
        disabled.setAttribute("text-anchor", "end");
        disabled.classList.add("node__disabled");
        disabled.textContent = t("canvas.nodeDisabled");
        group.appendChild(disabled);
      }

      const executionDetails: string[] = [];
      if (node.enabled === false) executionDetails.push(t("canvas.nodeDisabled"));
      if (node.condition) executionDetails.push(`${t("execution.condition")}: ${node.condition}`);
      if (node.breakpoint) executionDetails.push(t("execution.breakpoint"));
      if ((node.delay_before_ms ?? 0) > 0) {
        executionDetails.push(`${t("execution.delayBefore")}: ${node.delay_before_ms}`);
      }
      if ((node.delay_after_ms ?? 0) > 0) {
        executionDetails.push(`${t("execution.delayAfter")}: ${node.delay_after_ms}`);
      }
      if (node.continue_on_error) executionDetails.push(t("execution.continueOnError"));
      if ((node.timeout_ms ?? 0) > 0) executionDetails.push(`${t("execution.timeout")}: ${node.timeout_ms}`);
      if ((node.retry ?? 0) > 0) executionDetails.push(`${t("execution.retries")}: ${node.retry}`);
      if ((node.retry_delay_ms ?? 0) > 0) {
        executionDetails.push(`${t("execution.retryDelay")}: ${node.retry_delay_ms}`);
      }
      if (executionDetails.length > 0) {
        const details = document.createElementNS("http://www.w3.org/2000/svg", "title");
        details.textContent = executionDetails.join("\n");
        group.appendChild(details);
      }

      const inputs = descriptor?.inputs ?? [];
      const outputs = descriptor?.outputs ?? [];
      inputs.forEach((port, index) => {
        group.appendChild(
          this.makePort(node.id, port.name, port.display_name, port.value_type, "input", index),
        );
      });
      outputs.forEach((port, index) => {
        group.appendChild(
          this.makePort(node.id, port.name, port.display_name, port.value_type, "output", index),
        );
      });
      group.appendChild(this.makeExecutionPort(node.id, "exec", "Execution input", "input"));
      for (const [branch, label] of [
        ["always", t("inspector.edgeBranchAlways")],
        ["success", t("inspector.edgeBranchSuccess")],
        ["failure", t("inspector.edgeBranchFailure")],
      ] as const) {
        group.appendChild(
          this.makeExecutionPort(node.id, branch, label, "output", branch),
        );
      }

      group.addEventListener("pointerdown", (event) => {
        if (event.button !== 0 || this.spaceDown) return;
        if ((event.target as Element).closest(".port, .exec-port")) return;
        event.preventDefault();
        event.stopPropagation();
        if (event.ctrlKey || event.metaKey) {
          if (!this.toggleSelection(node.id)) return;
        } else if (event.shiftKey) {
          this.selectPathTo(node.id);
          if (!this.selected.has(node.id)) return;
        } else if (!this.selected.has(node.id)) {
          this.setSelection([node.id], node.id);
        } else {
          this.primarySelected = node.id;
          this.updateSelectionVisuals();
        }
        const point = this.toCanvas(event.clientX, event.clientY);
        const origins = new Map<string, { x: number; y: number }>();
        for (const id of this.selected) {
          const selectedNode = this.workflow.nodes.find((candidate) => candidate.id === id);
          if (!selectedNode) continue;
          origins.set(id, {
            x: selectedNode.position?.x ?? 0,
            y: selectedNode.position?.y ?? 0,
          });
        }
        this.drag = { primaryId: node.id, startX: point.x, startY: point.y, origins, moved: false };
        document.body.classList.add("is-canvas-dragging");
      });
      group.addEventListener("contextmenu", (event) => {
        this.showContextMenu(event, { kind: "node", id: node.id });
      });

      this.nodesLayer.appendChild(group);
    }
    this.updateSelectionVisuals();
  }
  /** Floating execution settings for one or many selected nodes. */
  private renderQuickConfig(): void {
    const nodes = this.workflow.nodes.filter((node) => this.selected.has(node.id));
    if (nodes.length === 0) {
      this.quickConfig.classList.add("is-hidden");
      this.quickConfig.replaceChildren();
      return;
    }
    this.quickConfig.classList.remove("is-hidden");
    this.quickConfig.replaceChildren();
    const title = document.createElement("div");
    title.className = "node-quick-config__title";
    title.textContent = t("canvas.quickConfig", { count: nodes.length });
    this.quickConfig.appendChild(title);

    const checkbox = (
      field: string,
      labelKey: string,
      values: boolean[],
      apply: (node: WorkflowNode, value: boolean) => void,
    ) => {
      const label = document.createElement("label");
      label.className = "node-quick-config__field node-quick-config__field--check";
      const input = document.createElement("input");
      input.type = "checkbox";
      input.dataset.field = field;
      const mixed = values.some((value) => value !== values[0]);
      input.checked = !mixed && values[0];
      input.indeterminate = mixed;
      input.addEventListener("change", () => {
        for (const node of nodes) apply(node, input.checked);
        this.handlers.onChange();
      });
      const text = document.createElement("span");
      text.textContent = t(labelKey);
      label.append(input, text);
      this.quickConfig.appendChild(label);
    };

    const number = (
      field: string,
      labelKey: string,
      values: number[],
      apply: (node: WorkflowNode, value: number) => void,
    ) => {
      const label = document.createElement("label");
      label.className = "node-quick-config__field";
      const text = document.createElement("span");
      text.textContent = t(labelKey);
      const input = document.createElement("input");
      input.className = "input input--small";
      input.type = "number";
      input.min = "0";
      input.dataset.field = field;
      const mixed = values.some((value) => value !== values[0]);
      input.value = mixed ? "" : String(values[0]);
      input.placeholder = mixed ? t("canvas.multipleValues") : "";
      input.addEventListener("change", () => {
        const value = Math.max(0, Number(input.value) || 0);
        for (const node of nodes) apply(node, value);
        this.handlers.onChange();
      });
      label.append(text, input);
      this.quickConfig.appendChild(label);
    };

    checkbox(
      "enabled",
      "execution.enabled",
      nodes.map((node) => node.enabled !== false),
      (node, value) => {
        if (value) delete node.enabled;
        else node.enabled = false;
      },
    );
    checkbox(
      "breakpoint",
      "execution.breakpoint",
      nodes.map((node) => node.breakpoint === true),
      (node, value) => {
        if (value) node.breakpoint = true;
        else delete node.breakpoint;
      },
    );

    const conditionValues = nodes.map((node) => node.condition ?? "");
    const conditionLabel = document.createElement("label");
    conditionLabel.className = "node-quick-config__field";
    const conditionText = document.createElement("span");
    conditionText.textContent = t("execution.condition");
    const condition = document.createElement("input");
    condition.className = "input input--small";
    condition.dataset.field = "condition";
    const conditionMixed = conditionValues.some((value) => value !== conditionValues[0]);
    condition.value = conditionMixed ? "" : conditionValues[0];
    condition.placeholder = conditionMixed
      ? t("canvas.multipleValues")
      : t("execution.conditionPlaceholder");
    condition.addEventListener("change", () => {
      const value = condition.value.trim();
      for (const node of nodes) {
        if (value) node.condition = value;
        else delete node.condition;
      }
      this.handlers.onChange();
    });
    conditionLabel.append(conditionText, condition);
    this.quickConfig.appendChild(conditionLabel);

    number(
      "delay_before_ms",
      "execution.delayBefore",
      nodes.map((node) => node.delay_before_ms ?? 0),
      (node, value) => {
        if (value > 0) node.delay_before_ms = value;
        else delete node.delay_before_ms;
      },
    );
    checkbox(
      "continue_on_error",
      "execution.continueOnError",
      nodes.map((node) => node.continue_on_error === true),
      (node, value) => {
        if (value) node.continue_on_error = true;
        else delete node.continue_on_error;
      },
    );
    number(
      "retry",
      "execution.retries",
      nodes.map((node) => node.retry ?? 0),
      (node, value) => {
        if (value > 0) node.retry = value;
        else delete node.retry;
      },
    );
    number(
      "timeout_ms",
      "execution.timeout",
      nodes.map((node) => node.timeout_ms ?? 0),
      (node, value) => {
        if (value > 0) node.timeout_ms = value;
        else delete node.timeout_ms;
      },
    );
    this.positionQuickConfig(nodes);
  }

  private positionQuickConfig(nodes: WorkflowNode[]): void {
    const wrap = this.svg.closest<HTMLElement>(".canvas-wrap") ?? this.svg.parentElement;
    if (!wrap) return;
    const wrapRect = wrap.getBoundingClientRect();
    const right = Math.max(...nodes.map((node) => (node.position?.x ?? 0) + NODE_WIDTH));
    const top = Math.min(...nodes.map((node) => node.position?.y ?? 0));
    const width = this.quickConfig.offsetWidth || 250;
    const screenRight = right * this.viewScale + this.viewX;
    const screenTop = top * this.viewScale + this.viewY;
    const left = screenRight + 12 + width > wrapRect.width
      ? Math.max(8, screenRight - width - NODE_WIDTH * this.viewScale - 24)
      : screenRight + 12;
    this.quickConfig.style.left = `${Math.max(8, left)}px`;
    this.quickConfig.style.top = `${Math.max(8, Math.min(wrapRect.height - 220, screenTop))}px`;
  }
  private makePort(
    nodeId: string,
    portName: string,
    displayName: string,
    valueType: string,
    kind: "input" | "output",
    index: number,
  ): SVGCircleElement {
    const circle = document.createElementNS("http://www.w3.org/2000/svg", "circle");
    const step = Math.min(22, (NODE_HEIGHT - 70) / Math.max(1, Math.ceil((kind === "input" ? 1 : 1))));
    const xOffset = kind === "output" ? NODE_WIDTH : 0;
    circle.setAttribute("cx", String(xOffset));
    circle.setAttribute("cy", String(32 + index * step));
    circle.setAttribute("r", "5.5");
    circle.classList.add("port", `port--${kind}`, "port--data");
    circle.dataset.nodeId = nodeId;
    circle.dataset.port = portName;
    circle.dataset.kind = kind;
    circle.dataset.portKind = "data";
    circle.dataset.valueType = valueType;
    const title = document.createElementNS("http://www.w3.org/2000/svg", "title");
    title.textContent = `${displayName} data ${kind === "input" ? "input" : "output"} · ${valueType}`;
    circle.appendChild(title);

    if (kind === "output") {
      circle.addEventListener("pointerdown", (event) => {
        if (event.button !== 0 || this.spaceDown) return;
        event.stopPropagation();
        const point = this.toCanvas(event.clientX, event.clientY);
        this.pendingDragMoved = false;
        this.pending = {
          sourceId: nodeId,
          sourcePort: portName,
          edgeKind: "data",
          x: point.x,
          y: point.y,
          originX: point.x,
          originY: point.y,
          mode: "drag",
        };
        this.pendingEdge.classList.remove("is-hidden");
        this.drawPendingEdge();
      });
      circle.addEventListener("click", (event) => {
        if (event.button !== 0 || this.spaceDown) return;
        event.stopPropagation();
        if (this.pendingDragMoved) {
          this.pendingDragMoved = false;
          return;
        }
        const point = this.toCanvas(event.clientX, event.clientY);
        this.pending = {
          sourceId: nodeId,
          sourcePort: portName,
          edgeKind: "data",
          x: point.x,
          y: point.y,
          originX: point.x,
          originY: point.y,
          mode: "click",
        };
        this.pendingEdge.classList.remove("is-hidden");
        this.drawPendingEdge();
      });
    } else {
      circle.addEventListener("pointerdown", (event) => {
        if (event.button !== 0 || !this.pending || this.pending.mode !== "click") return;
        event.stopPropagation();
        this.finishConnection(nodeId, portName);
      });
      circle.addEventListener("pointerup", (event) => {
        if (event.button !== 0 || !this.pending) return;
        event.stopPropagation();
        this.finishConnection(nodeId, portName);
      });
    }
    return circle;
  }

  private makeExecutionPort(
    nodeId: string,
    branch: "exec" | "always" | "success" | "failure",
    displayName: string,
    kind: "input" | "output",
    outputBranch?: "always" | "success" | "failure",
  ): SVGPolygonElement {
    const index = outputBranch === "success" ? 1 : outputBranch === "failure" ? 2 : 0;
    const x = kind === "input" ? 0 : NODE_WIDTH;
    const y = kind === "input" ? NODE_HEIGHT - 20 : NODE_HEIGHT - 52 + index * 16;
    const polygon = document.createElementNS("http://www.w3.org/2000/svg", "polygon");
    polygon.setAttribute(
      "points",
      `${x},${y - 7} ${x + 7},${y} ${x},${y + 7} ${x - 7},${y}`,
    );
    polygon.classList.add("exec-port", `exec-port--${kind}`, `exec-port--${branch}`);
    polygon.dataset.nodeId = nodeId;
    polygon.dataset.branch = branch;
    polygon.dataset.portKind = "execution";
    const title = document.createElementNS("http://www.w3.org/2000/svg", "title");
    title.textContent = `${displayName} execution ${kind === "input" ? "input" : "output"}`;
    polygon.appendChild(title);

    if (kind === "output") {
      const start = (branchName: "always" | "success" | "failure", event: PointerEvent, mode: "drag" | "click") => {
        if (event.button !== 0 || this.spaceDown) return;
        event.stopPropagation();
        const point = this.toCanvas(event.clientX, event.clientY);
        if (mode === "drag") this.pendingDragMoved = false;
        this.pending = {
          sourceId: nodeId,
          sourcePort: branchName,
          edgeKind: "control",
          x: point.x,
          y: point.y,
          originX: point.x,
          originY: point.y,
          mode,
        };
        this.pendingEdge.classList.remove("is-hidden");
        this.drawPendingEdge();
      };
      polygon.addEventListener("pointerdown", (event) => start(outputBranch!, event, "drag"));
      polygon.addEventListener("click", (event) => {
        if (this.pendingDragMoved) {
          this.pendingDragMoved = false;
          return;
        }
        start(outputBranch!, event, "click");
      });
    } else {
      polygon.addEventListener("pointerdown", (event) => {
        if (event.button !== 0 || !this.pending || this.pending.mode !== "click") return;
        event.stopPropagation();
        this.finishConnection(nodeId, "exec");
      });
      polygon.addEventListener("pointerup", (event) => {
        if (event.button !== 0 || !this.pending) return;
        event.stopPropagation();
        this.finishConnection(nodeId, "exec");
      });
    }
    return polygon;
  }

  private finishConnection(targetNodeId: string, targetPort: string): void {
    if (!this.pending) return;
    this.connect(
      this.pending.sourceId,
      targetNodeId,
      this.pending.sourcePort,
      targetPort,
      this.pending.edgeKind,
    );
    this.pending = null;
    this.pendingEdge.classList.add("is-hidden");
    this.pendingEdge.removeAttribute("d");
  }

  private connect(
    source: string,
    target: string,
    sourcePort: string,
    targetPort: string,
    kind: "control" | "data",
  ): void {
    if (source === target) {
      this.handlers.onStatus(t("canvas.selfConnection"));
      return;
    }
    if (edgeExists(this.workflow.edges, source, target, sourcePort, targetPort, kind)) {
      this.handlers.onStatus(t("canvas.duplicateConnection"));
      return;
    }
    const sourceNode = this.workflow.nodes.find((node) => node.id === source);
    const targetNode = this.workflow.nodes.find((node) => node.id === target);
    const sourceDescriptor = sourceNode ? this.handlers.descriptorFor(sourceNode.type) : undefined;
    const targetDescriptor = targetNode ? this.handlers.descriptorFor(targetNode.type) : undefined;
    const output = sourceDescriptor?.outputs.find((port) => port.name === sourcePort);
    const input = targetDescriptor?.inputs.find((port) => port.name === targetPort);
    if (kind === "data" && output && input && !valueTypesCompatible(output.value_type, input.value_type)) {
      this.handlers.onStatus(
        t("canvas.incompatiblePorts", {
          source: output.value_type,
          target: input.value_type,
        }),
      );
      return;
    }
    const base = {
      id: nextEdgeId(source, target, this.workflow.edges),
      kind,
      source,
      target,
    } as const;
    if (kind === "data") {
      this.workflow.edges.push({ ...base, source_port: sourcePort, target_port: targetPort });
    } else {
      const branch = sourcePort === "success" || sourcePort === "failure" ? sourcePort : undefined;
      this.workflow.edges.push(branch ? { ...base, branch } : { ...base });
    }
    this.handlers.onChange();
  }
  private renderEdges(): void {
    this.cancelAllEdgeAnimations();
    this.edgesLayer.replaceChildren();
    for (const edge of this.workflow.edges) {
      const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
      group.classList.add("edge-group", `edge-group--${edge.kind}`);
      group.dataset.edgeId = edge.id;
      group.dataset.edgeKind = edge.kind;

      const pathData = this.edgePath(edge);
      const hit = document.createElementNS("http://www.w3.org/2000/svg", "path");
      hit.classList.add("edge-hit");
      hit.setAttribute("d", pathData);
      hit.addEventListener("pointerdown", (event) => {
        event.stopPropagation();
        this.selectedEdge = edge.id;
        this.selected.clear();
        this.primarySelected = null;
        this.updateSelectionVisuals();
      });
      hit.addEventListener("contextmenu", (event) => {
        this.showContextMenu(event, { kind: "edge", id: edge.id });
      });

      const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
      path.classList.add("edge", `edge--${edge.kind}`);
      path.dataset.edgeId = edge.id;
      if (edge.kind === "control" && edge.condition) path.classList.add("edge--guarded");
      if (edge.kind === "control" && edge.branch && edge.branch !== "always") {
        path.classList.add(`edge--${edge.branch}`);
      }
      if (this.selectedEdge === edge.id) path.classList.add("edge--selected");
      const state = this.edgeStates.get(edge.id);
      if (state === "active") path.classList.add("edge--active");
      if (state === "data") path.classList.add("edge--data-active");
      path.setAttribute("d", pathData);
      const title = document.createElementNS("http://www.w3.org/2000/svg", "title");
      if (edge.kind === "data") {
        title.textContent = `${edge.source}.${edge.source_port ?? "out"} → ${edge.target}.${edge.target_port ?? "in"} (data)`;
      } else {
        const branch = edge.branch ?? "always";
        title.textContent = edge.condition
          ? `${edge.source} → ${edge.target} on ${branch} when ${edge.condition}`
          : `${edge.source} → ${edge.target} on ${branch}`;
      }
      path.appendChild(title);

      group.append(hit, path);
      const branchLabel = edge.kind === "control"
        ? edge.branch === "success"
          ? t("inspector.edgeBranchSuccess")
          : edge.branch === "failure"
            ? t("inspector.edgeBranchFailure")
            : ""
        : "";
      const visibleLabel = edge.kind === "control" ? edge.label?.trim() || branchLabel : "";
      if (visibleLabel) {
        const source = this.portCenter(edge.source, edge.kind, "output", edge.source_port ?? edge.branch);
        const target = this.portCenter(edge.target, edge.kind, "input", edge.target_port);
        const label = document.createElementNS("http://www.w3.org/2000/svg", "text");
        label.classList.add("edge__label");
        label.setAttribute("x", String((source.x + target.x) / 2));
        label.setAttribute("y", String((source.y + target.y) / 2 - 9));
        label.textContent = visibleLabel;
        group.appendChild(label);
      }
      this.edgesLayer.appendChild(group);
    }
    for (const [edgeId, state] of this.edgeStates) this.setEdgeState(edgeId, state);
  }

  private edgePath(edge: Workflow["edges"][number]): string {
    const source = this.portCenter(edge.source, edge.kind, "output", edge.source_port ?? edge.branch);
    const target = this.portCenter(edge.target, edge.kind, "input", edge.target_port);
    const dx = target.x - source.x;
    const dy = target.y - source.y;
    const distance = Math.hypot(dx, dy);
    const handle = Math.max(EDGE_MIN_HANDLE, Math.min(EDGE_MAX_HANDLE, distance * 0.45));
    if (Math.abs(dy) > Math.abs(dx) * 0.72) {
      const direction = Math.sign(dy) || 1;
      return `M ${source.x} ${source.y} C ${source.x} ${source.y + handle * direction}, ${target.x} ${target.y - handle * direction}, ${target.x} ${target.y}`;
    }
    const direction = dx >= 0 ? 1 : -0.65;
    return `M ${source.x} ${source.y} C ${source.x + handle * direction} ${source.y}, ${target.x - handle * direction} ${target.y}, ${target.x} ${target.y}`;
  }

  private portCenter(
    nodeId: string,
    edgeKind: "control" | "data",
    side: "input" | "output",
    port?: string,
  ): { x: number; y: number } {
    const node = this.workflow.nodes.find((candidate) => candidate.id === nodeId);
    const x = node?.position?.x ?? 0;
    const y = node?.position?.y ?? 0;
    const descriptor = node ? this.handlers.descriptorFor(node.type) : undefined;
    if (edgeKind === "data") {
      const ports = side === "output" ? descriptor?.outputs ?? [] : descriptor?.inputs ?? [];
      const index = Math.max(0, ports.findIndex((candidate) => candidate.name === port));
      return {
        x: side === "output" ? x + NODE_WIDTH : x,
        y: y + 32 + index * 22,
      };
    }
    if (side === "input") return { x, y: y + NODE_HEIGHT - 20 };
    const index = port === "success" ? 1 : port === "failure" ? 2 : 0;
    return { x: x + NODE_WIDTH, y: y + NODE_HEIGHT - 52 + index * 16 };
  }

  private drawPendingEdge(): void {
    if (!this.pending) return;
    const source = this.portCenter(
      this.pending.sourceId,
      this.pending.edgeKind,
      "output",
      this.pending.sourcePort,
    );
    const dx = this.pending.x - source.x;
    const dy = this.pending.y - source.y;
    const handle = Math.max(34, Math.min(120, Math.hypot(dx, dy) * 0.35));
    const path = Math.abs(dy) > Math.abs(dx) * 0.72
      ? `M ${source.x} ${source.y} C ${source.x} ${source.y + Math.sign(dy) * handle}, ${this.pending.x} ${this.pending.y - Math.sign(dy) * handle}, ${this.pending.x} ${this.pending.y}`
      : `M ${source.x} ${source.y} C ${source.x + handle} ${source.y}, ${this.pending.x - handle} ${this.pending.y}, ${this.pending.x} ${this.pending.y}`;
    this.pendingEdge.setAttribute("d", path);
  }
  private toCanvas(clientX: number, clientY: number): { x: number; y: number } {
    const rect = this.svg.getBoundingClientRect();
    return {
      x: (clientX - rect.left - this.viewX) / this.viewScale,
      y: (clientY - rect.top - this.viewY) / this.viewScale,
    };
  }

  private zoomBy(factor: number, anchor?: { x: number; y: number }): void {
    const rect = this.svg.getBoundingClientRect();
    const point = anchor ?? { x: rect.width / 2, y: rect.height / 2 };
    const world = this.toCanvas(rect.left + point.x, rect.top + point.y);
    const next = Math.max(0.2, Math.min(3, this.viewScale * factor));
    this.viewScale = next;
    this.viewX = point.x - world.x * next;
    this.viewY = point.y - world.y * next;
    this.applyView();
  }

  private applyView(): void {
    this.viewport.setAttribute(
      "transform",
      `translate(${this.viewX} ${this.viewY}) scale(${this.viewScale})`,
    );
    const wrap = this.svg.closest<HTMLElement>(".canvas-wrap");
    if (wrap) {
      const size = 24 * this.viewScale;
      wrap.style.setProperty("--grid-size", `${size}px`);
      wrap.style.setProperty("--grid-x", `${this.viewX % size}px`);
      wrap.style.setProperty("--grid-y", `${this.viewY % size}px`);
    }
    this.renderQuickConfig();
    this.handlers.onViewChange?.(this.viewScale);
  }
}