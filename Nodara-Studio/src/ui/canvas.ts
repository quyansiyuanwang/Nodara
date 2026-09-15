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

const NODE_WIDTH = 168;
const NODE_HEIGHT = 56;
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
  x: number;
  y: number;
  originX: number;
  originY: number;
  mode: "drag" | "click";
}

interface NodeDrag {
  nodeId: string;
  offsetX: number;
  offsetY: number;
  element: SVGGElement;
  moved: boolean;
}

type ContextTarget =
  | { kind: "node"; id: string }
  | { kind: "edge"; id: string };

export class Canvas {
  private readonly viewport: SVGGElement;
  private readonly nodesLayer: SVGGElement;
  private readonly edgesLayer: SVGGElement;
  private readonly pendingEdge: SVGPathElement;
  private selected: string | null = null;
  private selectedEdge: string | null = null;
  private pending: PendingConnection | null = null;
  private pendingDragMoved = false;
  private drag: NodeDrag | null = null;
  private paletteDragCleanup: (() => void) | null = null;
  private status: RunStatus | null = null;
  private readonly contextMenu: HTMLDivElement;
  private viewScale = 1;
  private viewX = 0;
  private viewY = 0;
  private pan: { clientX: number; clientY: number; viewX: number; viewY: number } | null = null;
  private spaceDown = false;

  constructor(
    private readonly svg: SVGSVGElement,
    private readonly workflow: Workflow,
    private readonly handlers: CanvasHandlers,
  ) {
    this.viewport = svg.querySelector("#viewport")!;
    this.nodesLayer = svg.querySelector("#nodes")!;
    this.edgesLayer = svg.querySelector("#edges")!;
    this.pendingEdge = svg.querySelector("#pending-edge")!;

    this.contextMenu = document.createElement("div");
    this.contextMenu.className = "context-menu";
    this.contextMenu.hidden = true;
    document.body.appendChild(this.contextMenu);
    document.addEventListener("pointerdown", (event) => {
      if (!this.contextMenu.hidden && !this.contextMenu.contains(event.target as Node)) {
        this.contextMenu.hidden = true;
      }
    });
    window.addEventListener("blur", () => {
      this.contextMenu.hidden = true;
    });

    // Listen on the window so node and port drags keep working even when the
    // pointer briefly leaves the SVG (a common cause of stuck interactions).
    window.addEventListener("pointermove", (event) => this.onPointerMove(event));
    window.addEventListener("pointerup", () => this.endInteraction());
    window.addEventListener("pointercancel", () => this.endInteraction());
    svg.addEventListener("pointerdown", (event) => {
      if (event.target !== svg) return;
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
      this.select(null);
    });
    svg.addEventListener(
      "wheel",
      (event) => {
        event.preventDefault();
        const rect = svg.getBoundingClientRect();
        this.zoomBy(Math.exp(-event.deltaY * 0.001), {
          x: event.clientX - rect.left,
          y: event.clientY - rect.top,
        });
      },
      { passive: false },
    );
    window.addEventListener("keydown", (event) => this.onKeyDown(event));
    window.addEventListener("keyup", (event) => {
      if (event.key === " ") this.spaceDown = false;
    });
    this.applyView();
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
        const source = this.nodeCenter(edge.source, "output");
        const target = this.nodeCenter(edge.target, "input");
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
    return this.selected;
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
      if (!known.has(edge.source) || !known.has(edge.target)) continue;
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
    const x = Math.max(24, center.x - NODE_WIDTH / 2 + jitter - stagger);
    const y = Math.max(24, center.y - NODE_HEIGHT / 2 + jitter - stagger);
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
        x: Math.round(x + stackOffset) % 1400,
        y: Math.round(y + stackOffset) % 800,
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
      const node = this.workflow.nodes.find((candidate) => candidate.id === this.drag!.nodeId);
      if (!node) return;
      const x = Math.max(0, Math.round(point.x - this.drag.offsetX));
      const y = Math.max(0, Math.round(point.y - this.drag.offsetY));
      if (node.position?.x === x && node.position?.y === y) return;
      node.position = { x, y };
      this.drag.moved = true;
      // Moving the existing group keeps pointer capture stable; re-rendering
      // the whole node list on every frame also made text selection flicker.
      this.drag.element.setAttribute("transform", `translate(${x}, ${y})`);
      this.renderEdges();
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
      if (!this.selected) return;
      event.preventDefault();
      this.duplicateNode(this.selected);
      return;
    }
    if (event.key === "F9" && this.selected) {
      event.preventDefault();
      this.toggleBreakpoint(this.selected);
      return;
    }
    if (event.key !== "Delete" && event.key !== "Backspace") return;
    this.deleteSelection();
  }

  private toggleBreakpoint(id: string): void {
    const node = this.workflow.nodes.find((candidate) => candidate.id === id);
    if (!node) return;
    if (node.breakpoint) delete node.breakpoint;
    else node.breakpoint = true;
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

  private showContextMenu(event: MouseEvent, target: ContextTarget): void {
    event.preventDefault();
    event.stopPropagation();
    if (target.kind === "node") {
      this.select(target.id);
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
          if (node.enabled === false) delete node.enabled;
          else node.enabled = false;
          this.contextMenu.hidden = true;
          this.handlers.onChange();
        });
        this.contextMenu.appendChild(toggle);
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
      if (this.selected === node.id) group.classList.add("node--selected");
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
          this.makePort(
            node.id,
            port.name,
            port.display_name,
            port.value_type,
            "input",
            index,
            inputs.length,
            0,
          ),
        );
      });
      outputs.forEach((port, index) => {
        group.appendChild(
          this.makePort(
            node.id,
            port.name,
            port.display_name,
            port.value_type,
            "output",
            index,
            outputs.length,
            NODE_WIDTH,
          ),
        );
      });

      group.addEventListener("pointerdown", (event) => {
        if (event.button !== 0 || this.spaceDown) return;
        if ((event.target as Element).classList.contains("port")) return;
        event.preventDefault();
        event.stopPropagation();
        const point = this.toCanvas(event.clientX, event.clientY);
        this.drag = {
          nodeId: node.id,
          offsetX: point.x - x,
          offsetY: point.y - y,
          element: group,
          moved: false,
        };
        document.body.classList.add("is-canvas-dragging");
        this.select(node.id);
      });
      group.addEventListener("contextmenu", (event) => {
        this.showContextMenu(event, { kind: "node", id: node.id });
      });

      this.nodesLayer.appendChild(group);
    }
  }

  private makePort(
    nodeId: string,
    portName: string,
    displayName: string,
    valueType: string,
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
    circle.dataset.valueType = valueType;
    const title = document.createElementNS("http://www.w3.org/2000/svg", "title");
    title.textContent = `${displayName} · ${valueType}`;
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
        this.connect(this.pending.sourceId, nodeId, this.pending.sourcePort, portName);
        this.pending = null;
        this.pendingEdge.classList.add("is-hidden");
        this.pendingEdge.removeAttribute("d");
      });
      circle.addEventListener("pointerup", (event) => {
        if (event.button !== 0 || !this.pending) return;
        event.stopPropagation();
        this.connect(this.pending.sourceId, nodeId, this.pending.sourcePort, portName);
        this.pending = null;
        this.pendingEdge.classList.add("is-hidden");
        this.pendingEdge.removeAttribute("d");
      });
    }
    return circle;
  }

  private connect(source: string, target: string, sourcePort: string, targetPort: string): void {
    if (source === target) {
      this.handlers.onStatus(t("canvas.selfConnection"));
      return;
    }
    if (edgeExists(this.workflow.edges, source, target, sourcePort, targetPort)) {
      this.handlers.onStatus(t("canvas.duplicateConnection"));
      return;
    }
    const sourceNode = this.workflow.nodes.find((node) => node.id === source);
    const targetNode = this.workflow.nodes.find((node) => node.id === target);
    const sourceDescriptor = sourceNode ? this.handlers.descriptorFor(sourceNode.type) : undefined;
    const targetDescriptor = targetNode ? this.handlers.descriptorFor(targetNode.type) : undefined;
    const output = sourceDescriptor?.outputs.find((port) => port.name === sourcePort);
    const input = targetDescriptor?.inputs.find((port) => port.name === targetPort);
    if (output && input && !valueTypesCompatible(output.value_type, input.value_type)) {
      this.handlers.onStatus(
        t("canvas.incompatiblePorts", {
          source: output.value_type,
          target: input.value_type,
        }),
      );
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
      const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
      group.classList.add("edge-group");
      group.dataset.edgeId = edge.id;

      const pathData = this.edgePath(edge.source, edge.target);
      const hit = document.createElementNS("http://www.w3.org/2000/svg", "path");
      hit.classList.add("edge-hit");
      hit.setAttribute("d", pathData);
      hit.addEventListener("pointerdown", (event) => {
        event.stopPropagation();
        this.select(null, edge.id);
      });
      hit.addEventListener("contextmenu", (event) => {
        this.showContextMenu(event, { kind: "edge", id: edge.id });
      });

      const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
      path.classList.add("edge");
      path.dataset.edgeId = edge.id;
      if (edge.condition) path.classList.add("edge--guarded");
      if (edge.branch && edge.branch !== "always") {
        path.classList.add(`edge--${edge.branch}`);
      }
      if (this.selectedEdge === edge.id) path.classList.add("edge--selected");
      path.setAttribute("d", pathData);
      path.appendChild(document.createElementNS("http://www.w3.org/2000/svg", "title"));
      const branchLabel = edge.branch === "success"
        ? t("inspector.edgeBranchSuccess")
        : edge.branch === "failure"
          ? t("inspector.edgeBranchFailure")
          : "";
      const tooltip = edge.condition
        ? t("canvas.edgeCondition", {
            source: edge.source,
            target: edge.target,
            condition: edge.condition,
          })
        : t("canvas.edge", { source: edge.source, target: edge.target });
      path.lastChild!.textContent = branchLabel ? `${branchLabel}: ${tooltip}` : tooltip;

      group.append(hit, path);
      const visibleLabel = edge.label?.trim() || branchLabel;
      if (visibleLabel) {
        const source = this.nodeCenter(edge.source, "output");
        const target = this.nodeCenter(edge.target, "input");
        const label = document.createElementNS("http://www.w3.org/2000/svg", "text");
        label.classList.add("edge__label");
        label.setAttribute("x", String((source.x + target.x) / 2));
        label.setAttribute("y", String((source.y + target.y) / 2 - 9));
        label.textContent = visibleLabel;
        group.appendChild(label);
      }
      this.edgesLayer.appendChild(group);
    }
  }

  private edgePath(sourceId: string, targetId: string): string {
    const source = this.nodeCenter(sourceId, "output");
    const target = this.nodeCenter(targetId, "input");
    const dx = target.x - source.x;
    const dy = target.y - source.y;
    const handle = Math.max(
      EDGE_MIN_HANDLE,
      Math.min(EDGE_MAX_HANDLE, Math.abs(dx) * 0.45 + Math.abs(dy) * 0.16 + 20),
    );
    // Bend the final tangent towards the vertical gap. The arrowhead then
    // follows the visual approach instead of staying horizontal on tall links.
    const vertical = Math.sign(dy) * Math.min(Math.abs(dy) * 0.35, handle * 0.65);
    return `M ${source.x} ${source.y} C ${source.x + handle} ${source.y + vertical * 0.38}, ${target.x - handle} ${target.y - vertical}, ${target.x} ${target.y}`;
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
    this.handlers.onViewChange?.(this.viewScale);
  }
}
