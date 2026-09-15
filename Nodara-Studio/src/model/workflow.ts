/**
 * The document model the editor manipulates.
 *
 * It is intentionally identical to what the runtime consumes, so export is
 * `JSON.stringify` and import is `JSON.parse` — no translation layer to drift.
 */

import {
  NodeDescriptor,
  SCHEMA_VERSION,
  Workflow,
  WorkflowEdge,
  WorkflowNode,
} from "../runtime/types";

/**
 * Location of the published workflow schema, relative to a document that lives
 * in this repository (`examples/`, `workflow/`).
 *
 * A document that declares `$schema` gets node-type completion, configuration
 * completion, inline documentation, defaults and enums from any editor that
 * understands JSON Schema. The Studio therefore treats it as part of the
 * document rather than as disposable metadata.
 */
export const WORKFLOW_SCHEMA_PATH = "../Nodara-Core/schema/workflow.schema.json";

export function emptyWorkflow(): Workflow {
  return {
    $schema: WORKFLOW_SCHEMA_PATH,
    schema_version: SCHEMA_VERSION,
    id: `workflow.${Date.now().toString(36)}`,
    metadata: { name: "Untitled workflow", tags: [] },
    nodes: [
      { id: "start", type: "core.Start", label: "Start", config: {}, position: { x: 80, y: 160 } },
      { id: "end", type: "core.End", label: "End", config: {}, position: { x: 720, y: 160 } },
    ],
    edges: [],
    variables: {},
  };
}

/**
 * A runnable starter graph shown on first launch and when creating a new file.
 * `emptyWorkflow` remains the minimal two-node scaffold used by tests and
 * programmatic callers.
 */
export function starterWorkflow(): Workflow {
  const workflow = emptyWorkflow();
  workflow.metadata.name = "Hello workflow";
  workflow.nodes = [
    { id: "start", type: "core.Start", label: "Start", config: {}, position: { x: 80, y: 160 } },
    {
      id: "hello",
      type: "core.Log",
      label: "Log message",
      config: { message: "Hello from Nodara", level: "info" },
      position: { x: 400, y: 160 },
    },
    { id: "end", type: "core.End", label: "End", config: { code: 0 }, position: { x: 720, y: 160 } },
  ];
  workflow.edges = [
    { id: "start-hello", source: "start", target: "hello" },
    { id: "hello-end", source: "hello", target: "end" },
  ];
  return workflow;
}

/**
 * Copy a document into the editor's live instance.
 *
 * The Studio mutates one workflow object in place because the canvas, the
 * inspector and the event log all hold a reference to it. This is the single
 * place that decides what an imported, exported or agent-proposed document
 * means, and it deliberately keeps `$schema`: dropping it would silently turn
 * off the content hints every other editor derives from the reference.
 */
export function applyWorkflow(target: Workflow, incoming: Partial<Workflow> | null): Workflow {
  const next = incoming ?? {};
  target.$schema = next.$schema || WORKFLOW_SCHEMA_PATH;
  target.schema_version = next.schema_version ?? SCHEMA_VERSION;
  target.id = next.id ?? "workflow.untitled";
  target.metadata = next.metadata ?? { name: "Untitled workflow", tags: [] };
  target.nodes = next.nodes ?? [];
  target.edges = next.edges ?? [];
  target.variables = next.variables ?? {};
  return target;
}

export interface NodeTypeAdmission {
  allowed: boolean;
  reason?: string;
}

/**
 * Editor-level admission rules for single-instance scaffold nodes.
 *
 * Runtime validation remains authoritative, but rejecting a second Start at the
 * point of insertion gives immediate feedback and keeps the graph unambiguous.
 */
export function nodeTypeAdmission(
  workflow: Workflow,
  nodeType: string,
): NodeTypeAdmission {
  if (
    nodeType === "core.Start" &&
    workflow.nodes.some((node) => node.type === "core.Start")
  ) {
    return {
      allowed: false,
      reason: "only one core.Start node is allowed per workflow",
    };
  }
  return { allowed: true };
}

/** Document-level admission rules shared by import, JSON apply and agent plans. */
export function workflowAdmission(
  workflow: Pick<Workflow, "nodes">,
): NodeTypeAdmission {
  const startNodes = workflow.nodes.filter((node) => node.type === "core.Start");
  if (startNodes.length > 1) {
    return {
      allowed: false,
      reason: `workflow declares ${startNodes.length} core.Start nodes; only one is allowed`,
    };
  }
  return { allowed: true };
}

let nodeCounter = 0;

/** A unique node id derived from the node type, readable in a diff. */
export function nextNodeId(descriptor: NodeDescriptor, existing: WorkflowNode[]): string {
  const stem = descriptor.node_type
    .split(".")
    .pop()!
    .replace(/[^A-Za-z0-9]/g, "")
    .toLowerCase() || "node";
  let candidate = stem;
  while (existing.some((node) => node.id === candidate)) {
    nodeCounter += 1;
    candidate = `${stem}${nodeCounter}`;
  }
  return candidate;
}

let edgeCounter = 0;

/** A unique edge id. */
export function nextEdgeId(source: string, target: string, existing: WorkflowEdge[]): string {
  let candidate = `${source}-${target}`;
  while (existing.some((edge) => edge.id === candidate)) {
    edgeCounter += 1;
    candidate = `${source}-${target}-${edgeCounter}`;
  }
  return candidate;
}

/** True when the same pair of ports is already connected. */
/** Whether an output value can feed an input port according to the descriptors. */
export function valueTypesCompatible(source: string, target: string): boolean {
  return (
    source === target ||
    source === "any" ||
    target === "any" ||
    (source === "string" && target === "path") ||
    (source === "path" && target === "string")
  );
}

export function edgeExists(
  edges: WorkflowEdge[],
  source: string,
  target: string,
  sourcePort?: string,
  targetPort?: string,
): boolean {
  return edges.some(
    (edge) =>
      edge.source === source &&
      edge.target === target &&
      (edge.source_port ?? "out") === (sourcePort ?? "out") &&
      (edge.target_port ?? "in") === (targetPort ?? "in"),
  );
}

/** Build the default configuration for a newly added node. */
export function defaultConfig(descriptor: NodeDescriptor): Record<string, unknown> {
  const config: Record<string, unknown> = {};
  const properties = descriptor.config_schema?.properties ?? {};
  for (const [key, schema] of Object.entries(properties)) {
    if (schema.default !== undefined) {
      config[key] = schema.default;
    }
  }
  return config;
}

/**
 * Validate a document locally before spending a round trip.
 *
 * This is *not* a replacement for `POST /workflows/validate`; it only catches
 * the mistakes that are obvious while dragging, so the editor can give instant
 * feedback.
 */
export function localProblems(workflow: Workflow): string[] {
  const problems: string[] = [];
  const ids = new Set<string>();
  for (const node of workflow.nodes) {
    if (!node.id) problems.push("a node has no id");
    if (ids.has(node.id)) problems.push(`duplicate node id \`${node.id}\``);
    ids.add(node.id);
    if (!node.type) problems.push(`node \`${node.id}\` has no type`);
  }
  const edgeIds = new Set<string>();
  for (const edge of workflow.edges) {
    if (edgeIds.has(edge.id)) problems.push(`duplicate edge id \`${edge.id}\``);
    edgeIds.add(edge.id);
    if (!ids.has(edge.source)) problems.push(`edge \`${edge.id}\` has a missing source`);
    if (!ids.has(edge.target)) problems.push(`edge \`${edge.id}\` has a missing target`);
  }
  const startNodes = workflow.nodes.filter((node) => node.type === "core.Start");
  if (startNodes.length === 0) {
    problems.push("no core.Start node");
  } else if (startNodes.length > 1) {
    problems.push(`workflow declares ${startNodes.length} core.Start nodes; only one is allowed`);
  }
  if (!workflow.nodes.some((node) => node.type === "core.End")) {
    problems.push("no core.End node");
  }
  problems.push(...cycleProblems(workflow));
  return problems;
}

/**
 * Detect directed cycles with Kahn's algorithm.
 *
 * A workflow must be a DAG; the runtime rejects cycles on validate, but the
 * editor can say so the moment an edge is drawn instead of after a round trip.
 */
function cycleProblems(workflow: Workflow): string[] {
  const indegree = new Map<string, number>();
  const adjacency = new Map<string, string[]>();
  for (const node of workflow.nodes) {
    indegree.set(node.id, 0);
    adjacency.set(node.id, []);
  }
  for (const edge of workflow.edges) {
    if (!indegree.has(edge.source) || !indegree.has(edge.target)) continue;
    adjacency.get(edge.source)!.push(edge.target);
    indegree.set(edge.target, (indegree.get(edge.target) ?? 0) + 1);
  }
  const ready = [...indegree.entries()].filter(([, degree]) => degree === 0).map(([id]) => id);
  let visited = 0;
  while (ready.length > 0) {
    const id = ready.pop()!;
    visited += 1;
    for (const next of adjacency.get(id) ?? []) {
      const degree = indegree.get(next)! - 1;
      indegree.set(next, degree);
      if (degree === 0) ready.push(next);
    }
  }
  if (visited === indegree.size) return [];
  const stuck = [...indegree.entries()]
    .filter(([, degree]) => degree > 0)
    .map(([id]) => `\`${id}\``)
    .join(", ");
  return [`the workflow contains a cycle involving ${stuck}`];
}
