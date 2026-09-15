import type { Workflow } from "../runtime/types";

export const STUDIO_GROUPS_EXTENSION = "studio.groups";

export const GROUP_COLORS = [
  "#5b8def",
  "#3fb27f",
  "#e0a33e",
  "#e06060",
  "#9b7de8",
  "#42b8c6",
] as const;

export interface WorkflowGroup {
  id: string;
  name: string;
  color: string;
  node_ids: string[];
}

function normalizeGroup(value: unknown): WorkflowGroup | null {
  if (!value || typeof value !== "object") return null;
  const candidate = value as Record<string, unknown>;
  if (typeof candidate.id !== "string" || typeof candidate.name !== "string") return null;
  const nodeIds = Array.isArray(candidate.node_ids)
    ? candidate.node_ids.filter((id): id is string => typeof id === "string")
    : [];
  const color = typeof candidate.color === "string" ? candidate.color : GROUP_COLORS[0];
  return {
    id: candidate.id,
    name: candidate.name,
    color,
    node_ids: [...new Set(nodeIds)],
  };
}

export function getWorkflowGroups(workflow: Workflow): WorkflowGroup[] {
  const raw = workflow.metadata.extensions?.[STUDIO_GROUPS_EXTENSION];
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((value) => {
    const group = normalizeGroup(value);
    return group ? [group] : [];
  });
}

export function setWorkflowGroups(workflow: Workflow, groups: WorkflowGroup[]): void {
  const extensions = workflow.metadata.extensions ?? (workflow.metadata.extensions = {});
  const normalized = groups
    .map((group) => normalizeGroup(group))
    .filter((group): group is WorkflowGroup => group !== null);
  if (normalized.length > 0) {
    extensions[STUDIO_GROUPS_EXTENSION] = normalized;
  } else {
    delete extensions[STUDIO_GROUPS_EXTENSION];
  }
  if (Object.keys(extensions).length === 0) delete workflow.metadata.extensions;
}

export function createWorkflowGroup(
  workflow: Workflow,
  nodeIds: string[],
  name: string,
  color: string = GROUP_COLORS[0],
): WorkflowGroup {
  const validIds = new Set(workflow.nodes.map((node) => node.id));
  const members = [...new Set(nodeIds)].filter((id) => validIds.has(id));
  const groups = getWorkflowGroups(workflow);
  let index = 1;
  while (groups.some((group) => group.id === `group-${index}`)) index += 1;
  const group: WorkflowGroup = {
    id: `group-${index}`,
    name: name.trim() || `Group ${index}`,
    color,
    node_ids: members,
  };
  setWorkflowGroups(workflow, [...groups, group]);
  assignNodesToGroup(workflow, group.id, members);
  return group;
}

export function assignNodesToGroup(workflow: Workflow, groupId: string, nodeIds: string[]): void {
  const validIds = new Set(workflow.nodes.map((node) => node.id));
  const selected = new Set(nodeIds.filter((id) => validIds.has(id)));
  const groups = getWorkflowGroups(workflow).map((group) => ({
    ...group,
    node_ids: group.node_ids.filter((id) => !selected.has(id)),
  }));
  const target = groups.find((group) => group.id === groupId);
  if (!target) return;
  target.node_ids = [...new Set([...target.node_ids, ...selected])];
  setWorkflowGroups(workflow, groups.filter((group) => group.node_ids.length > 0));
}

export function removeNodesFromGroups(workflow: Workflow, nodeIds: string[]): void {
  const selected = new Set(nodeIds);
  const groups = getWorkflowGroups(workflow)
    .map((group) => ({
      ...group,
      node_ids: group.node_ids.filter((id) => !selected.has(id)),
    }))
    .filter((group) => group.node_ids.length > 0);
  setWorkflowGroups(workflow, groups);
}

export function renameWorkflowGroup(workflow: Workflow, groupId: string, name: string): void {
  const groups = getWorkflowGroups(workflow).map((group) =>
    group.id === groupId ? { ...group, name: name.trim() || group.name } : group,
  );
  setWorkflowGroups(workflow, groups);
}

export function setWorkflowGroupColor(workflow: Workflow, groupId: string, color: string): void {
  const groups = getWorkflowGroups(workflow).map((group) =>
    group.id === groupId ? { ...group, color } : group,
  );
  setWorkflowGroups(workflow, groups);
}

export function deleteWorkflowGroup(workflow: Workflow, groupId: string): void {
  setWorkflowGroups(workflow, getWorkflowGroups(workflow).filter((group) => group.id !== groupId));
}

export function groupForNode(workflow: Workflow, nodeId: string): WorkflowGroup | undefined {
  return getWorkflowGroups(workflow).find((group) => group.node_ids.includes(nodeId));
}

export function renameNodeInGroups(workflow: Workflow, previousId: string, nextId: string): void {
  const groups = getWorkflowGroups(workflow).map((group) => ({
    ...group,
    node_ids: group.node_ids.map((id) => (id === previousId ? nextId : id)),
  }));
  setWorkflowGroups(workflow, groups);
}

export function pruneWorkflowGroups(workflow: Workflow): void {
  const validIds = new Set(workflow.nodes.map((node) => node.id));
  const groups = getWorkflowGroups(workflow)
    .map((group) => ({
      ...group,
      node_ids: [...new Set(group.node_ids.filter((id) => validIds.has(id)))],
    }))
    .filter((group) => group.node_ids.length > 0);
  setWorkflowGroups(workflow, groups);
}

export function groupBoundingBox(
  workflow: Workflow,
  group: WorkflowGroup,
  nodeWidth: number,
  nodeHeight: number,
  padding = { x: 20, top: 34, bottom: 18 },
): { x: number; y: number; width: number; height: number } | null {
  const members = group.node_ids
    .map((id) => workflow.nodes.find((node) => node.id === id))
    .filter((node): node is NonNullable<typeof node> => node !== undefined);
  if (members.length === 0) return null;
  const minX = Math.min(...members.map((node) => node.position?.x ?? 0));
  const minY = Math.min(...members.map((node) => node.position?.y ?? 0));
  const maxX = Math.max(...members.map((node) => (node.position?.x ?? 0) + nodeWidth));
  const maxY = Math.max(...members.map((node) => (node.position?.y ?? 0) + nodeHeight));
  return {
    x: minX - padding.x,
    y: minY - padding.top,
    width: maxX - minX + padding.x * 2,
    height: maxY - minY + padding.top + padding.bottom,
  };
}
