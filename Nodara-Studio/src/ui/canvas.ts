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
import {
  createWorkflowGroup,
  deleteWorkflowGroup,
  getWorkflowGroups,
  groupBoundingBox,
  groupForNode,
  pruneWorkflowGroups,
  removeNodesFromGroups,
  renameWorkflowGroup,
} from "../model/groups";
import { NodeDescriptor, RunStatus, Workflow, WorkflowNode } from "../runtime/types";
import {
  edgeColor,
  nodeColor,
  removeEdgeVisual,
  removeNodeVisual,
} from "../model/visuals";

const NODE_WIDTH = 200;
const NODE_HEIGHT = 108;
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
  mode: "drag" | "click" | "choose";
  dual: boolean;
  dataSourcePort?: string;
}

interface DataPortPair {
  sourcePort: string;
  targetPort: string;
  sourceLabel: string;
  targetLabel: string;
}

interface NodeDrag {
  primaryId: string;
  startX: number;
  startY: number;
  origins: Map<string, { x: number; y: number }>;
  moved: boolean;
  revealQuickConfig: boolean;
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
  | { kind: "edge"; id: string }
  | { kind: "group"; id: string };

export class Canvas {
  private readonly viewport: SVGGElement;
  private readonly nodesLayer: SVGGElement;
  private readonly edgesLayer: SVGGElement;
  private readonly groupsLayer: SVGGElement;
  private readonly pendingEdge: SVGPathElement;
  private readonly pendingDataEdge: SVGPathElement;
  private readonly selectionBox: SVGRectElement;
  private readonly quickConfig: HTMLDivElement;
  private readonly edgeAnimations = new Map<string, EdgeAnimation>();
  private quickConfigVisible = false;
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
    this.groupsLayer = document.createElementNS("http://www.w3.org/2000/svg", "g");
    this.groupsLayer.classList.add("groups");
    this.viewport.insertBefore(this.groupsLayer, this.edgesLayer);
    this.pendingEdge = svg.querySelector("#pending-edge")!;
    this.pendingDataEdge = this.pendingEdge.cloneNode(false) as SVGPathElement;
    this.pendingDataEdge.id = "pending-data-edge";
    this.pendingDataEdge.classList.add("edge--pending-data");
    this.pendingEdge.after(this.pendingDataEdge);
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
    if (status === "running") {
      // Resume the direction markers when a paused run continues.
      for (const [edgeId, state] of this.edgeStates) this.setEdgeState(edgeId, state);
      return;
    }
    if (status === "paused") {
      // Paused runs keep their execution snapshot visible for inspection.
      this.cancelAllEdgeAnimations();
      return;
    }
    // Pending and terminal runs must leave no execution-only colors or arrows
    // behind. Freeze is for a paused run only; completed, failed and cancelled
    // runs return the canvas to its authored document appearance.
    this.clearStates();
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
      element.classList.remove("node--running", "node--done", "node--failed", "node--active", "node--ready");
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
      runner.setAttribute("points", "-5,-3.5 7,0 -5,3.5");
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
        const duration = Math.max(1_700, Math.min(4_200, length * 8));
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
    this.setSelection(nodeId ? [nodeId] : [], nodeId, false);
    this.selectedEdge = edgeId;
    this.updateSelectionVisuals();
    this.handlers.onSelect(this.primarySelected);
  }

  selectedNodeIds(): string[] {
    return [...this.selected];
  }

  /** Select an explicit set of node identifiers. */
  selectNodes(ids: string[]): void {
    const valid = ids.filter((id) => this.workflow.nodes.some((node) => node.id === id));
    if (valid.length > 0) this.setSelection(valid, valid[0]);
  }

  /** Select every node in a persistent group. */
  selectGroup(groupId: string): void {
    const group = getWorkflowGroups(this.workflow).find((candidate) => candidate.id === groupId);
    if (!group) return;
    const ids = group.node_ids.filter((id) => this.workflow.nodes.some((node) => node.id === id));
    if (ids.length > 0) this.setSelection(ids, ids[0]);
  }

  /** Create a persistent group from the current selection. */
  createGroupFromSelection(name: string): string | null {
    const ids = [...this.selected];
    if (ids.length === 0) return null;
    const group = createWorkflowGroup(this.workflow, ids, name);
    this.handlers.onChange();
    this.selectGroup(group.id);
    return group.id;
  }

  /** Remove the current selection from all persistent groups. */
  removeSelectionFromGroups(): void {
    if (this.selected.size === 0) return;
    removeNodesFromGroups(this.workflow, [...this.selected]);
    this.handlers.onChange();
  }

  /** Identifier of the group containing the primary selected node, if any. */
  selectedGroupId(): string | null {
    const nodeId = this.primarySelected ?? this.selected.values().next().value ?? null;
    return nodeId ? groupForNode(this.workflow, nodeId)?.id ?? null : null;
  }

  private setSelection(
    ids: Iterable<string>,
    primary: string | null,
    notify = true,
  ): void {
    this.quickConfigVisible = false;
    this.selected = new Set(ids);
    this.primarySelected = primary && this.selected.has(primary)
      ? primary
      : this.selected.values().next().value ?? null;
    this.selectedEdge = null;
    this.updateSelectionVisuals();
    if (notify) this.handlers.onSelect(this.primarySelected);
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
    for (const group of getWorkflowGroups(this.workflow)) {
      const element = this.groupsLayer.querySelector<SVGGElement>(
        `[data-group-id="${group.id}"]`,
      );
      element?.classList.toggle(
        "node-group--selected",
        group.node_ids.length > 0 && group.node_ids.every((id) => this.selected.has(id)),
      );
    }
    // Re-rendering the document must not notify the shell again: doing so
    // discarded focused Inspector inputs on every keystroke.
    if (!this.quickConfig.contains(document.activeElement)) this.renderQuickConfig();
  }

  private toggleSelection(nodeId: string): boolean {
    if (this.selected.has(nodeId)) {
      this.selected.delete(nodeId);
      if (this.primarySelected === nodeId) {
        this.primarySelected = this.selected.values().next().value ?? null;
      }
      this.updateSelectionVisuals();
      this.handlers.onSelect(this.primarySelected);
      return false;
    }
    this.selected.add(nodeId);
    this.primarySelected = nodeId;
    this.updateSelectionVisuals();
    this.handlers.onSelect(this.primarySelected);
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
    this.renderGroups();
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
        if (this.quickConfigVisible) this.renderQuickConfig();
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
    if (this.pending && this.pending.mode !== "choose") {
      this.clearPendingConnection();
    }
    if (this.drag) {
      const drag = this.drag;
      this.drag = null;
      document.body.classList.remove("is-canvas-dragging");
      if (drag.moved) {
        this.handlers.onChange();
      } else if (drag.revealQuickConfig) {
        // A plain click reveals the floating execution settings. Dragging,
        // marquee selection and Ctrl/Shift multi-selection never open it.
        this.quickConfigVisible = true;
        this.renderQuickConfig();
      }
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
      this.clearPendingConnection();
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
      removeEdgeVisual(this.workflow, this.selectedEdge);
      this.workflow.edges = this.workflow.edges.filter((edge) => edge.id !== this.selectedEdge);
      this.select(null);
      this.handlers.onChange();
      return;
    }
    if (this.selected.size > 0) {
      const ids = new Set(this.selected);
      for (const id of ids) removeNodeVisual(this.workflow, id);
      this.workflow.nodes = this.workflow.nodes.filter((node) => !ids.has(node.id));
      this.workflow.edges = this.workflow.edges.filter(
        (edge) => !ids.has(edge.source) && !ids.has(edge.target),
      );
      pruneWorkflowGroups(this.workflow);
      this.select(null);
      this.handlers.onChange();
    }
  }

  private showContextMenu(event: MouseEvent, target: ContextTarget): void {
    event.preventDefault();
    event.stopPropagation();
    if (target.kind === "node") {
      if (!this.selected.has(target.id)) this.select(target.id);
    } else if (target.kind === "edge") {
      this.select(null, target.id);
    } else {
      this.selectGroup(target.id);
    }

    this.contextMenu.replaceChildren();
    const title = document.createElement("div");
    title.className = "context-menu__title";
    title.textContent = target.kind === "node"
      ? t("canvas.nodeTitle", { id: target.id })
      : target.kind === "edge"
        ? t("canvas.connectionTitle", { id: target.id })
        : t("groups.menuTitle");
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

        const membership = groupForNode(this.workflow, node.id);
        if (membership) {
          const selectGroup = document.createElement("button");
          selectGroup.type = "button";
          selectGroup.className = "context-menu__item";
          selectGroup.dataset.action = "select-group";
          selectGroup.textContent = t("groups.selectAll");
          selectGroup.addEventListener("click", () => {
            this.contextMenu.hidden = true;
            this.selectGroup(membership.id);
          });
          this.contextMenu.appendChild(selectGroup);

          const removeFromGroup = document.createElement("button");
          removeFromGroup.type = "button";
          removeFromGroup.className = "context-menu__item";
          removeFromGroup.dataset.action = "remove-from-group";
          removeFromGroup.textContent = t("groups.removeMembers");
          removeFromGroup.addEventListener("click", () => {
            removeNodesFromGroups(this.workflow, [...this.selected]);
            this.contextMenu.hidden = true;
            this.handlers.onChange();
          });
          this.contextMenu.appendChild(removeFromGroup);
        } else {
          const createGroup = document.createElement("button");
          createGroup.type = "button";
          createGroup.className = "context-menu__item";
          createGroup.dataset.action = "create-group";
          createGroup.textContent = t("groups.create");
          createGroup.addEventListener("click", () => {
            const name = t("groups.defaultName", { index: getWorkflowGroups(this.workflow).length + 1 });
            this.createGroupFromSelection(name);
            this.contextMenu.hidden = true;
          });
          this.contextMenu.appendChild(createGroup);
        }
      }
    }

    if (target.kind === "group") {
      const group = getWorkflowGroups(this.workflow).find((candidate) => candidate.id === target.id);
      if (group) {
        const select = document.createElement("button");
        select.type = "button";
        select.className = "context-menu__item";
        select.dataset.action = "select-group";
        select.textContent = t("groups.selectAll");
        select.addEventListener("click", () => {
          this.contextMenu.hidden = true;
          this.selectGroup(group.id);
        });
        this.contextMenu.appendChild(select);

        const rename = document.createElement("button");
        rename.type = "button";
        rename.className = "context-menu__item";
        rename.dataset.action = "rename-group";
        rename.textContent = t("groups.rename");
        rename.addEventListener("click", () => {
          const next = window.prompt(t("groups.renamePrompt"), group.name)?.trim();
          if (next) {
            renameWorkflowGroup(this.workflow, group.id, next);
            this.handlers.onChange();
          }
          this.contextMenu.hidden = true;
        });
        this.contextMenu.appendChild(rename);

        const removeGroup = document.createElement("button");
        removeGroup.type = "button";
        removeGroup.className = "context-menu__item context-menu__item--danger";
        removeGroup.dataset.action = "delete-group";
        removeGroup.textContent = t("groups.removeGroup");
        removeGroup.addEventListener("click", () => {
          deleteWorkflowGroup(this.workflow, group.id);
          this.contextMenu.hidden = true;
          this.handlers.onChange();
        });
        this.contextMenu.appendChild(removeGroup);
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

    if (target.kind !== "group") {
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
    }

    this.contextMenu.hidden = false;
    const bounds = this.contextMenu.getBoundingClientRect();
    const left = Math.max(8, Math.min(event.clientX, window.innerWidth - bounds.width - 8));
    const top = Math.max(8, Math.min(event.clientY, window.innerHeight - bounds.height - 8));
    this.contextMenu.style.left = `${left}px`;
    this.contextMenu.style.top = `${top}px`;
  }

  private renderGroups(): void {
    this.groupsLayer.replaceChildren();
    for (const group of getWorkflowGroups(this.workflow)) {
      const ids = group.node_ids.filter((id) => this.workflow.nodes.some((node) => node.id === id));
      if (ids.length === 0) continue;
      const box = groupBoundingBox(this.workflow, group, NODE_WIDTH, NODE_HEIGHT);
      if (!box) continue;

      const element = document.createElementNS("http://www.w3.org/2000/svg", "g");
      element.classList.add("node-group");
      element.dataset.groupId = group.id;
      element.style.setProperty("--group-color", group.color);
      if (ids.every((id) => this.selected.has(id))) element.classList.add("node-group--selected");

      const body = document.createElementNS("http://www.w3.org/2000/svg", "rect");
      body.setAttribute("x", String(box.x));
      body.setAttribute("y", String(box.y));
      body.setAttribute("width", String(box.width));
      body.setAttribute("height", String(box.height));
      body.setAttribute("rx", "12");
      body.classList.add("node-group__body");

      const header = document.createElementNS("http://www.w3.org/2000/svg", "path");
      header.setAttribute(
        "d",
        `M ${box.x} ${box.y + 28} V ${box.y + 12} Q ${box.x} ${box.y} ${box.x + 12} ${box.y} H ${box.x + box.width - 12} Q ${box.x + box.width} ${box.y} ${box.x + box.width} ${box.y + 12} V ${box.y + 28} Z`,
      );
      header.classList.add("node-group__header");

      const label = document.createElementNS("http://www.w3.org/2000/svg", "text");
      label.setAttribute("x", String(box.x + 12));
      label.setAttribute("y", String(box.y + 18));
      label.classList.add("node-group__label");
      label.textContent = group.name;

      const count = document.createElementNS("http://www.w3.org/2000/svg", "text");
      count.setAttribute("x", String(box.x + box.width - 12));
      count.setAttribute("y", String(box.y + 18));
      count.setAttribute("text-anchor", "end");
      count.classList.add("node-group__count");
      count.textContent = t("groups.nodeCount", { count: ids.length });

      element.append(body, header, label, count);
      element.addEventListener("pointerdown", (event) => {
        if (event.button !== 0 || this.spaceDown) return;
        event.preventDefault();
        event.stopPropagation();
        this.setSelection(ids, ids[0]);
        const point = this.toCanvas(event.clientX, event.clientY);
        const origins = new Map<string, { x: number; y: number }>();
        for (const id of ids) {
          const node = this.workflow.nodes.find((candidate) => candidate.id === id);
          if (!node) continue;
          origins.set(id, { x: node.position?.x ?? 0, y: node.position?.y ?? 0 });
        }
        this.drag = {
          primaryId: ids[0],
          startX: point.x,
          startY: point.y,
          origins,
          moved: false,
          revealQuickConfig: false,
        };
        document.body.classList.add("is-canvas-dragging");
      });
      element.addEventListener("contextmenu", (event) => {
        this.showContextMenu(event, { kind: "group", id: group.id });
      });
      this.groupsLayer.appendChild(element);
    }
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
      const customColor = nodeColor(this.workflow, node.id);
      if (customColor) group.style.setProperty("--node-color", customColor);

      const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
      rect.setAttribute("width", String(NODE_WIDTH));
      rect.setAttribute("height", String(NODE_HEIGHT));
      rect.setAttribute("rx", "7");
      rect.classList.add("node__body");
      group.appendChild(rect);

      const label = document.createElementNS("http://www.w3.org/2000/svg", "text");
      label.setAttribute("x", "10");
      label.setAttribute("y", "18");
      label.classList.add("node__label");
      label.textContent = this.truncateNodeText(
        node.label ?? descriptor?.display_name ?? node.type,
        25,
      );
      group.appendChild(label);

      const type = document.createElementNS("http://www.w3.org/2000/svg", "text");
      type.setAttribute("x", "10");
      type.setAttribute("y", "34");
      type.classList.add("node__type");
      type.textContent = this.truncateNodeText(node.type, 29);
      group.appendChild(type);

      const summaryText = this.nodeSummary(node);
      if (summaryText) {
        const summary = document.createElementNS("http://www.w3.org/2000/svg", "text");
        summary.setAttribute("x", "10");
        summary.setAttribute("y", "52");
        summary.classList.add("node__summary");
        summary.textContent = summaryText;
        group.appendChild(summary);
      }

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
        if (event.altKey) {
          event.preventDefault();
          event.stopPropagation();
          const point = this.toCanvas(event.clientX, event.clientY);
          this.pending = {
            sourceId: node.id,
            sourcePort: "always",
            edgeKind: "control",
            x: point.x,
            y: point.y,
            originX: point.x,
            originY: point.y,
            mode: "drag",
            dual: true,
            dataSourcePort: undefined,
          };
          this.pendingEdge.classList.remove("is-hidden");
          this.drawPendingEdge();
          return;
        }
        this.quickConfigVisible = false;
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
          this.handlers.onSelect(this.primarySelected);
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
        this.drag = {
          primaryId: node.id,
          startX: point.x,
          startY: point.y,
          origins,
          moved: false,
          revealQuickConfig: !(event.ctrlKey || event.metaKey || event.shiftKey),
        };
        document.body.classList.add("is-canvas-dragging");
      });
      group.addEventListener("pointerup", (event) => {
        if (!this.pending || (event.target as Element).closest(".port, .exec-port")) return;
        event.preventDefault();
        event.stopPropagation();
        this.finishConnectionOnNode(node.id, event.clientX, event.clientY);
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
    if (!this.quickConfigVisible || nodes.length === 0) {
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
    const step = 16;
    const xOffset = kind === "output" ? NODE_WIDTH : 0;
    circle.setAttribute("cx", String(xOffset));
    circle.setAttribute("cy", String(27 + index * step));
    circle.setAttribute("r", "5");
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
          dual: event.altKey,
          dataSourcePort: event.altKey ? portName : undefined,
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
          dual: event.altKey,
          dataSourcePort: event.altKey ? portName : undefined,
        };
        this.pendingEdge.classList.remove("is-hidden");
        this.drawPendingEdge();
      });
    } else {
      circle.addEventListener("pointerdown", (event) => {
        if (event.button !== 0 || !this.pending || this.pending.mode !== "click") return;
        event.stopPropagation();
        if (this.pending.dual) this.finishDualConnection(nodeId, portName);
        else this.finishConnection(nodeId, portName);
      });
      circle.addEventListener("pointerup", (event) => {
        if (event.button !== 0 || !this.pending) return;
        event.stopPropagation();
        if (this.pending.dual) this.finishDualConnection(nodeId, portName);
        else this.finishConnection(nodeId, portName);
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
    const y = kind === "input" ? NODE_HEIGHT - 14 : NODE_HEIGHT - 40 + index * 13;
    const polygon = document.createElementNS("http://www.w3.org/2000/svg", "polygon");
    polygon.setAttribute(
      "points",
      `${x},${y - 6} ${x + 6},${y} ${x},${y + 6} ${x - 6},${y}`,
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
          dual: event.altKey,
          dataSourcePort: undefined,
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
        if (this.pending.dual) this.finishDualConnection(nodeId, "exec");
        else this.finishConnection(nodeId, "exec");
      });
      polygon.addEventListener("pointerup", (event) => {
        if (event.button !== 0 || !this.pending) return;
        event.stopPropagation();
        if (this.pending.dual) this.finishDualConnection(nodeId, "exec");
        else this.finishConnection(nodeId, "exec");
      });
    }
    return polygon;
  }

  private clearPendingConnection(): void {
    this.pending = null;
    this.pendingEdge.classList.add("is-hidden");
    this.pendingEdge.removeAttribute("d");
    this.pendingDataEdge.classList.add("is-hidden");
    this.pendingDataEdge.removeAttribute("d");
    this.contextMenu.hidden = true;
  }

  private finishConnection(targetNodeId: string, targetPort: string): void {
    if (!this.pending) return;
    const pending = this.pending;
    this.connect(
      pending.sourceId,
      targetNodeId,
      pending.sourcePort,
      targetPort,
      pending.edgeKind,
    );
    this.clearPendingConnection();
  }

  private finishConnectionOnNode(targetNodeId: string, clientX: number, clientY: number): void {
    if (!this.pending) return;
    if (this.pending.dual) {
      this.finishDualConnection(targetNodeId, undefined, clientX, clientY);
      return;
    }
    if (this.pending.edgeKind === "control") {
      this.finishConnection(targetNodeId, "exec");
      return;
    }
    const pairs = this.dataPortPairs(targetNodeId);
    if (pairs.length === 0) {
      this.handlers.onStatus(t("canvas.noCompatibleTarget"));
      this.clearPendingConnection();
      return;
    }
    if (pairs.length === 1) {
      this.finishConnection(targetNodeId, pairs[0].targetPort);
      return;
    }
    this.showDataChooser(targetNodeId, pairs, clientX, clientY, false);
  }

  private finishDualConnection(
    targetNodeId: string,
    targetPort?: string,
    clientX?: number,
    clientY?: number,
  ): void {
    const pending = this.pending;
    if (!pending) return;
    const controlSource = pending.edgeKind === "control" ? pending.sourcePort : "always";
    const dataPairs = this.dataPortPairs(
      targetNodeId,
      pending.edgeKind === "data" ? targetPort : undefined,
    );
    if (dataPairs.length === 0) {
      const changed = this.connect(
        pending.sourceId,
        targetNodeId,
        controlSource,
        "exec",
        "control",
        false,
      );
      if (changed) this.handlers.onChange();
      this.handlers.onStatus(t("canvas.controlOnly"));
      this.clearPendingConnection();
      return;
    }
    if (dataPairs.length > 1) {
      this.showDataChooser(
        targetNodeId,
        dataPairs,
        clientX ?? this.pending!.x * this.viewScale + this.viewX,
        clientY ?? this.pending!.y * this.viewScale + this.viewY,
        true,
      );
      return;
    }
    this.applyDualConnection(targetNodeId, controlSource, dataPairs[0]);
  }

  private applyDualConnection(
    targetNodeId: string,
    controlSource: string,
    pair: DataPortPair,
  ): void {
    const pending = this.pending;
    if (!pending) return;
    const controlChanged = this.connect(
      pending.sourceId,
      targetNodeId,
      controlSource,
      "exec",
      "control",
      false,
    );
    const dataChanged = this.connect(
      pending.sourceId,
      targetNodeId,
      pair.sourcePort,
      pair.targetPort,
      "data",
      false,
    );
    if (controlChanged || dataChanged) this.handlers.onChange();
    if (!dataChanged) this.handlers.onStatus(t("canvas.controlOnly"));
    this.clearPendingConnection();
  }

  private dataPortPairs(targetNodeId: string, targetPort?: string): DataPortPair[] {
    const pending = this.pending;
    if (!pending) return [];
    const sourceNode = this.workflow.nodes.find((node) => node.id === pending.sourceId);
    const targetNode = this.workflow.nodes.find((node) => node.id === targetNodeId);
    const sourceDescriptor = sourceNode ? this.handlers.descriptorFor(sourceNode.type) : undefined;
    const targetDescriptor = targetNode ? this.handlers.descriptorFor(targetNode.type) : undefined;
    if (!sourceDescriptor || !targetDescriptor) return [];
    const outputs = sourceDescriptor.outputs.filter(
      (port) => !pending.dataSourcePort || port.name === pending.dataSourcePort,
    );
    const inputs = targetDescriptor.inputs.filter(
      (port) => !targetPort || port.name === targetPort,
    );
    const usedInputs = new Set(
      this.workflow.edges
        .filter((edge) => edge.kind === "data" && edge.target === targetNodeId)
        .map((edge) => edge.target_port),
    );
    const pairs: Array<DataPortPair & { score: number }> = [];
    outputs.forEach((output, outputIndex) => {
      inputs.forEach((input, inputIndex) => {
        if (!valueTypesCompatible(output.value_type, input.value_type)) return;
        const exact = output.value_type !== "any" && output.value_type === input.value_type ? 1 : 0;
        const unused = usedInputs.has(input.name) ? 0 : 1;
        pairs.push({
          sourcePort: output.name,
          targetPort: input.name,
          sourceLabel: output.display_name,
          targetLabel: input.display_name,
          score: exact * 100 + unused * 10 - outputIndex - inputIndex * 0.01,
        });
      });
    });
    return pairs
      .sort((left, right) => right.score - left.score)
      .map(({ score: _score, ...pair }) => pair);
  }

  private showDataChooser(
    targetNodeId: string,
    pairs: DataPortPair[],
    clientX: number,
    clientY: number,
    dual: boolean,
  ): void {
    if (!this.pending) return;
    this.pending.mode = "choose";
    this.contextMenu.replaceChildren();
    const title = document.createElement("p");
    title.className = "context-menu__title";
    title.textContent = t("canvas.chooseDataPort");
    this.contextMenu.appendChild(title);
    for (const pair of pairs) {
      const choice = document.createElement("button");
      choice.type = "button";
      choice.className = "context-menu__item";
      choice.textContent = `${pair.sourceLabel} → ${pair.targetLabel}`;
      choice.addEventListener("click", () => {
        if (dual) {
          const controlSource = this.pending?.edgeKind === "control"
            ? this.pending.sourcePort
            : "always";
          this.applyDualConnection(targetNodeId, controlSource, pair);
        } else {
          this.connect(
            this.pending!.sourceId,
            targetNodeId,
            pair.sourcePort,
            pair.targetPort,
            "data",
          );
          this.clearPendingConnection();
        }
      });
      this.contextMenu.appendChild(choice);
    }
    this.contextMenu.style.left = `${Math.min(window.innerWidth - 240, clientX)}px`;
    this.contextMenu.style.top = `${Math.min(window.innerHeight - 160, clientY)}px`;
    this.contextMenu.hidden = false;
  }

  private connect(
    source: string,
    target: string,
    sourcePort: string,
    targetPort: string,
    kind: "control" | "data",
    notify = true,
  ): boolean {
    if (source === target) {
      this.handlers.onStatus(t("canvas.selfConnection"));
      return false;
    }
    if (edgeExists(this.workflow.edges, source, target, sourcePort, targetPort, kind)) {
      this.handlers.onStatus(t("canvas.duplicateConnection"));
      return false;
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
      return false;
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
    if (notify) this.handlers.onChange();
    return true;
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
        this.handlers.onSelect(null);
      });
      hit.addEventListener("contextmenu", (event) => {
        this.showContextMenu(event, { kind: "edge", id: edge.id });
      });

      const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
      path.classList.add("edge", `edge--${edge.kind}`);
      path.dataset.edgeId = edge.id;
      const customColor = edgeColor(this.workflow, edge.id);
      if (customColor) path.style.setProperty("--edge-color", customColor);
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

      const idle = document.createElementNS("http://www.w3.org/2000/svg", "path");
      idle.classList.add("edge-idle", `edge-idle--${edge.kind}`);
      idle.setAttribute("d", pathData);
      group.append(hit, path, idle);
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
        y: y + 27 + index * 16,
      };
    }
    if (side === "input") return { x, y: y + NODE_HEIGHT - 14 };
    const index = port === "success" ? 1 : port === "failure" ? 2 : 0;
    return { x: x + NODE_WIDTH, y: y + NODE_HEIGHT - 40 + index * 13 };
  }

  private truncateNodeText(value: string, limit: number): string {
    return value.length <= limit ? value : `${value.slice(0, Math.max(1, limit - 1))}…`;
  }

  private nodeSummary(node: WorkflowNode): string | null {
    const config = node.config as Record<string, unknown> | undefined;
    if (!config) return null;

    const region = ["x", "y", "width", "height"].every((key) => config[key] !== undefined);
    if (region) {
      return this.truncateNodeText(
        `${String(config.x)},${String(config.y)} · ${String(config.width)}×${String(config.height)}`,
        31,
      );
    }

    const preferred = [
      "message",
      "value",
      "text",
      "command",
      "program",
      "url",
      "path",
      "name",
      "level",
    ];
    const secret = /(password|secret|token|api[_-]?key|credential)/i;
    const keys = [
      ...preferred.filter((key) => config[key] !== undefined),
      ...Object.keys(config).filter((key) => !preferred.includes(key)),
    ];
    for (const key of keys) {
      if (secret.test(key)) continue;
      const value = config[key];
      const rendered = this.formatNodeValue(value);
      if (rendered) return this.truncateNodeText(`${key}: ${rendered}`, 31);
    }
    return null;
  }

  private formatNodeValue(value: unknown): string {
    if (value === null || value === undefined) return "";
    if (typeof value === "string") return value.replace(/\s+/g, " ").trim();
    if (typeof value === "number" || typeof value === "boolean") return String(value);
    if (Array.isArray(value)) return `[${value.length}]`;
    return "{…}";
  }

  private pendingPath(
    source: { x: number; y: number },
    end: { x: number; y: number },
  ): string {
    const dx = end.x - source.x;
    const dy = end.y - source.y;
    const handle = Math.max(34, Math.min(120, Math.hypot(dx, dy) * 0.35));
    return Math.abs(dy) > Math.abs(dx) * 0.72
      ? `M ${source.x} ${source.y} C ${source.x} ${source.y + Math.sign(dy) * handle}, ${end.x} ${end.y - Math.sign(dy) * handle}, ${end.x} ${end.y}`
      : `M ${source.x} ${source.y} C ${source.x + handle} ${source.y}, ${end.x - handle} ${end.y}, ${end.x} ${end.y}`;
  }

  private drawPendingEdge(): void {
    if (!this.pending) return;
    const end = { x: this.pending.x, y: this.pending.y };

    // A data-only drag must originate from the data output the operator
    // grabbed. The previous implementation always drew the primary preview
    // from the Always execution port, which made the line appear to snap to a
    // control socket even though the eventual edge was a data edge.
    if (this.pending.edgeKind === "data" && !this.pending.dual) {
      const dataSourcePort = this.pending.dataSourcePort ?? this.pending.sourcePort;
      const dataSource = this.portCenter(
        this.pending.sourceId,
        "data",
        "output",
        dataSourcePort,
      );
      this.pendingEdge.classList.add("edge--pending-data");
      this.pendingEdge.setAttribute("d", this.pendingPath(dataSource, end));
      this.pendingDataEdge.classList.add("is-hidden");
      this.pendingDataEdge.removeAttribute("d");
      return;
    }

    this.pendingEdge.classList.remove("edge--pending-data");
    const controlSourcePort = this.pending.edgeKind === "control"
      ? this.pending.sourcePort
      : "always";
    const controlSource = this.portCenter(
      this.pending.sourceId,
      "control",
      "output",
      controlSourcePort,
    );
    this.pendingEdge.setAttribute("d", this.pendingPath(controlSource, end));

    if (!this.pending.dual) {
      this.pendingDataEdge.classList.add("is-hidden");
      this.pendingDataEdge.removeAttribute("d");
      return;
    }

    const sourceNode = this.workflow.nodes.find((node) => node.id === this.pending?.sourceId);
    const descriptor = sourceNode ? this.handlers.descriptorFor(sourceNode.type) : undefined;
    const dataSourcePort = this.pending.dataSourcePort ?? descriptor?.outputs[0]?.name;
    if (!dataSourcePort) {
      this.pendingDataEdge.classList.add("is-hidden");
      return;
    }
    const dataSource = this.portCenter(
      this.pending.sourceId,
      "data",
      "output",
      dataSourcePort,
    );
    this.pendingDataEdge.setAttribute("d", this.pendingPath(dataSource, end));
    this.pendingDataEdge.classList.remove("is-hidden");
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
    if (!this.quickConfig.contains(document.activeElement)) this.renderQuickConfig();
    this.handlers.onViewChange?.(this.viewScale);
  }
}