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
  if (!workflow.nodes.some((node) => node.type === "core.Start")) {
    problems.push("no core.Start node");
  }
  if (!workflow.nodes.some((node) => node.type === "core.End")) {
    problems.push("no core.End node");
  }
  return problems;
}
