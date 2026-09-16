import type { Workflow, WorkflowEdge, WorkflowNode } from "../runtime/types";

export interface WorkflowDiffEntry {
  id: string;
  label: string;
  detail?: string;
}

export interface WorkflowDiff {
  nodesAdded: WorkflowDiffEntry[];
  nodesRemoved: WorkflowDiffEntry[];
  nodesChanged: WorkflowDiffEntry[];
  edgesAdded: WorkflowDiffEntry[];
  edgesRemoved: WorkflowDiffEntry[];
  edgesChanged: WorkflowDiffEntry[];
}

function nodeDetail(node: WorkflowNode): string {
  const execution = [
    node.enabled === false ? "disabled" : "",
    node.breakpoint ? "breakpoint" : "",
    node.condition ? `condition=${node.condition}` : "",
  ].filter(Boolean);
  return execution.join(" · ");
}

function edgeDetail(edge: WorkflowEdge): string {
  if (edge.kind === "data") {
    return `data ${edge.source}.${edge.source_port ?? "out"} → ${edge.target}.${edge.target_port ?? "in"}`;
  }
  return `control ${edge.source} → ${edge.target} · ${edge.branch ?? "always"}`;
}

export function diffWorkflows(base: Workflow | undefined, next: Workflow): WorkflowDiff {
  const beforeNodes = new Map((base?.nodes ?? []).map((node) => [node.id, node]));
  const afterNodes = new Map(next.nodes.map((node) => [node.id, node]));
  const beforeEdges = new Map((base?.edges ?? []).map((edge) => [edge.id, edge]));
  const afterEdges = new Map(next.edges.map((edge) => [edge.id, edge]));

  const nodesAdded: WorkflowDiffEntry[] = [];
  const nodesRemoved: WorkflowDiffEntry[] = [];
  const nodesChanged: WorkflowDiffEntry[] = [];
  for (const [id, node] of afterNodes) {
    const previous = beforeNodes.get(id);
    const label = node.label ?? node.type;
    if (!previous) nodesAdded.push({ id, label, detail: node.type });
    else if (JSON.stringify(previous) !== JSON.stringify(node)) {
      nodesChanged.push({ id, label, detail: nodeDetail(node) || node.type });
    }
  }
  for (const [id, node] of beforeNodes) {
    if (!afterNodes.has(id)) nodesRemoved.push({ id, label: node.label ?? node.type });
  }

  const edgesAdded: WorkflowDiffEntry[] = [];
  const edgesRemoved: WorkflowDiffEntry[] = [];
  const edgesChanged: WorkflowDiffEntry[] = [];
  for (const [id, edge] of afterEdges) {
    const previous = beforeEdges.get(id);
    const label = `${edge.source} → ${edge.target}`;
    if (!previous) edgesAdded.push({ id, label, detail: edgeDetail(edge) });
    else if (JSON.stringify(previous) !== JSON.stringify(edge)) {
      edgesChanged.push({ id, label, detail: edgeDetail(edge) });
    }
  }
  for (const [id, edge] of beforeEdges) {
    if (!afterEdges.has(id)) {
      edgesRemoved.push({ id, label: `${edge.source} → ${edge.target}` });
    }
  }

  return { nodesAdded, nodesRemoved, nodesChanged, edgesAdded, edgesRemoved, edgesChanged };
}

export function isWorkflowDiffEmpty(diff: WorkflowDiff): boolean {
  return Object.values(diff).every((entries) => entries.length === 0);
}
