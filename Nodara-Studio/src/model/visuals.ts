import type { Workflow, WorkflowEdge, WorkflowNode } from "../runtime/types";

export type ThemeId =
  | "obsidian"
  | "graphite"
  | "ocean"
  | "ember"
  | "paper"
  | "high-contrast";

export const THEME_PRESETS: ReadonlyArray<{ id: ThemeId; label: string }> = [
  { id: "obsidian", label: "Obsidian" },
  { id: "graphite", label: "Graphite" },
  { id: "ocean", label: "Ocean" },
  { id: "ember", label: "Ember" },
  { id: "paper", label: "Paper" },
  { id: "high-contrast", label: "High contrast" },
];

export const NODE_COLORS = ["#5b8def", "#3fb27f", "#e0a33e", "#e06060", "#9b7cf6", "#3aa6b9", "#d878b0"] as const;
export const EDGE_COLORS = ["#8b93a7", "#58a6ff", "#45b97c", "#e05d68", "#e9c96f", "#9b7cf6", "#3aa6b9"] as const;

interface VisualExtension {
  version: 1;
  nodes?: Record<string, { color?: string }>;
  edges?: Record<string, { color?: string }>;
}

function visuals(workflow: Workflow, create = false): VisualExtension {
  workflow.metadata.extensions ??= {};
  const existing = workflow.metadata.extensions["studio.visuals"];
  if (isVisualExtension(existing)) return existing;
  if (!create) return { version: 1 };
  const created: VisualExtension = { version: 1 };
  workflow.metadata.extensions["studio.visuals"] = created;
  return created;
}

function isVisualExtension(value: unknown): value is VisualExtension {
  return typeof value === "object" && value !== null && (value as { version?: unknown }).version === 1;
}

export function nodeColor(workflow: Workflow, nodeId: string): string | undefined {
  return visuals(workflow).nodes?.[nodeId]?.color;
}

export function edgeColor(workflow: Workflow, edgeId: string): string | undefined {
  return visuals(workflow).edges?.[edgeId]?.color;
}

export function setNodeColor(workflow: Workflow, nodeId: string, color: string | null): void {
  const style = visuals(workflow, true);
  style.nodes ??= {};
  if (color) style.nodes[nodeId] = { color };
  else delete style.nodes[nodeId];
  if (Object.keys(style.nodes).length === 0) delete style.nodes;
  cleanup(workflow);
}

export function setEdgeColor(workflow: Workflow, edgeId: string, color: string | null): void {
  const style = visuals(workflow, true);
  style.edges ??= {};
  if (color) style.edges[edgeId] = { color };
  else delete style.edges[edgeId];
  if (Object.keys(style.edges).length === 0) delete style.edges;
  cleanup(workflow);
}

function cleanup(workflow: Workflow): void {
  const style = visuals(workflow);
  if (!style.nodes && !style.edges) delete workflow.metadata.extensions?.["studio.visuals"];
}

export function renameNodeVisual(workflow: Workflow, previous: string, next: string): void {
  const style = visuals(workflow);
  const value = style.nodes?.[previous];
  if (!value) return;
  style.nodes ??= {};
  delete style.nodes[previous];
  style.nodes[next] = value;
}

export function removeNodeVisual(workflow: Workflow, nodeId: string): void {
  const style = visuals(workflow);
  if (!style.nodes?.[nodeId]) return;
  delete style.nodes[nodeId];
  cleanup(workflow);
}

export function removeEdgeVisual(workflow: Workflow, edgeId: string): void {
  const style = visuals(workflow);
  if (!style.edges?.[edgeId]) return;
  delete style.edges[edgeId];
  cleanup(workflow);
}

export function applyTheme(theme: ThemeId): void {
  document.documentElement.dataset.theme = theme;
  try {
    localStorage.setItem("nodara.theme", theme);
  } catch {
    // Storage is optional; the current session still uses the selected theme.
  }
}

export function storedTheme(): ThemeId {
  try {
    const value = localStorage.getItem("nodara.theme");
    if (THEME_PRESETS.some((theme) => theme.id === value)) return value as ThemeId;
  } catch {
    // Storage is optional.
  }
  return "obsidian";
}

export function visualNodeStyle(node: WorkflowNode, workflow: Workflow): string {
  const color = nodeColor(workflow, node.id);
  return color ? `--node-color:${color}` : "";
}

export function visualEdgeStyle(edge: WorkflowEdge, workflow: Workflow): string {
  const color = edgeColor(workflow, edge.id);
  return color ? `--edge-color:${color}` : "";
}
