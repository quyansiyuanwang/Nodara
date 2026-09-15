/**
 * Wire types mirrored from the runtime protocol.
 *
 * These are deliberately hand-written rather than generated: they are the
 * contract the Studio depends on, and keeping them explicit makes a breaking
 * change visible in review. They match `nodara-schema` and `nodara-runtime` one for one.
 */

export const API_VERSION = "v1";
export const SCHEMA_VERSION = "2.0";

export interface PortDescriptor {
  name: string;
  display_name: string;
  kind: "input" | "output";
  value_type: string;
  required: boolean;
  description?: string;
  default?: unknown;
}

/** JSON Schema subset the Studio renders configuration forms from. */
export interface JsonSchema {
  type?: string | string[];
  title?: string;
  description?: string;
  default?: unknown;
  enum?: unknown[];
  const?: unknown;
  examples?: unknown[];
  minimum?: number;
  maximum?: number;
  minLength?: number;
  maxLength?: number;
  pattern?: string;
  step?: number;
  format?: string;
  properties?: Record<string, JsonSchema>;
  required?: string[];
  additionalProperties?: boolean | JsonSchema;
  items?: JsonSchema;
  minItems?: number;
  maxItems?: number;
}

export interface NodeDescriptor {
  node_type: string;
  display_name: string;
  category: string;
  description: string;
  version: string;
  inputs: PortDescriptor[];
  outputs: PortDescriptor[];
  config_schema: JsonSchema;
  capabilities: string[];
  permissions: string[];
  dangerous: boolean;
  allows_additional_config: boolean;
  plugin_id?: string;
}

export interface WorkflowNode {
  id: string;
  type: string;
  label?: string;
  config: Record<string, unknown>;
  position?: { x: number; y: number };
  /** Disabled nodes act as transparent pass-throughs during execution. */
  enabled?: boolean;
  condition?: string;
  delay_before_ms?: number;
  delay_after_ms?: number;
  /** Continue through outgoing branches after all retries fail. */
  continue_on_error?: boolean;
  /** Maximum time the executor may spend per attempt, in milliseconds. */
  timeout_ms?: number;
  /** Additional attempts after the first failed execution. */
  retry?: number;
  retry_delay_ms?: number;
  /** Publish one output port under this run-scope variable. */
  result_var?: string;
  /** Output port selected by result_var; defaults to `out` or the first output. */
  result_port?: string;
}

export type EdgeBranch = "always" | "success" | "failure";

export interface WorkflowEdge {
  id: string;
  source: string;
  target: string;
  source_port?: string;
  target_port?: string;
  /** Selects whether the source node must succeed or fail to activate this edge. */
  branch?: EdgeBranch;
  condition?: string;
  label?: string;
}

export interface WorkflowVariable {
  value: unknown;
  description?: string;
  secret?: boolean;
}

export interface Workflow {
  /**
   * JSON Schema the document is written against.
   *
   * Editors resolve it to complete node types, configuration keys, defaults and
   * enums. It is part of the document: the Studio keeps it on round-trip so an
   * exported workflow still completes wherever it is opened.
   */
  $schema?: string;
  schema_version: string;
  id: string;
  metadata: {
    name: string;
    description?: string;
    tags: string[];
    author?: string;
    version?: string;
  };
  nodes: WorkflowNode[];
  edges: WorkflowEdge[];
  variables: Record<string, WorkflowVariable>;
}

export type Severity = "info" | "warning" | "error";

export interface Diagnostic {
  severity: Severity;
  code: string;
  message: string;
  path: string;
  node_id?: string;
  edge_id?: string;
  hint?: string;
}

export interface ValidationReport {
  diagnostics: Diagnostic[];
}

export type RunStatus =
  | "pending"
  | "running"
  | "paused"
  | "completed"
  | "failed"
  | "cancelled";

export interface RunSnapshot {
  id: string;
  workflow_id: string;
  status: RunStatus;
  started_at_ms: number;
  finished_at_ms?: number;
  nodes_executed: number;
  variables: Record<string, unknown>;
  failure?: { code: string; message: string };
  event_count: number;
  artifact_count?: number;
}

export type ExtensionKind =
  | "builtin"
  | "in_process"
  | "plugin"
  | "ui"
  | "policy"
  | "integration"
  | "other";

export interface ExtensionDescriptor {
  id: string;
  name: string;
  version: string;
  kind: ExtensionKind;
  source: string;
  description?: string;
  capabilities: string[];
  permissions: string[];
  node_types: string[];
  loaded: boolean;
}

export interface PluginFeature {
  id: string;
  name: string;
  kind: ExtensionKind;
  description?: string;
  capabilities: string[];
  permissions: string[];
  node_types: string[];
}

export interface PluginSummary {
  id: string;
  name: string;
  version: string;
  protocol_version: string;
  capabilities: string[];
  permissions: string[];
  node_types: string[];
  features: PluginFeature[];
  description?: string;
  loaded: boolean;
}

export interface PluginListResponse {
  plugins: PluginSummary[];
  failures: { id: string; message: string }[];
}

export type ExecutionEvent =
  | { type: "run_started"; workflow_id: string }
  | { type: "node_started"; node_id: string; node_type: string }
  | { type: "node_progress"; node_id: string; progress?: number; message?: string }
  | {
      type: "node_finished";
      node_id: string;
      outputs: Record<string, unknown>;
      duration_ms: number;
    }
  | {
      type: "node_failed";
      node_id: string;
      code: string;
      message: string;
      retryable: boolean;
    }
  | { type: "log"; level: "debug" | "info" | "warn" | "error"; message: string; node_id?: string }
  | { type: "run_paused" }
  | { type: "run_resumed" }
  | { type: "run_cancelled"; reason?: string }
  | { type: "run_completed"; nodes_executed: number; duration_ms: number }
  | { type: "run_failed"; code: string; message: string }
  | {
      type: "capability_decision";
      capability: string;
      decision: string;
      node_id?: string;
    };

export interface EventEnvelope {
  run_id: string;
  seq: number;
  timestamp_ms: number;
  event: ExecutionEvent;
}

export interface ApiErrorBody {
  code: string;
  message: string;
  detail?: unknown;
}

/* --- Agent sessions ------------------------------------------------------ */

export type SessionStatus =
  | "draft"
  | "planning"
  | "awaiting_approval"
  | "ready"
  | "running"
  | "completed"
  | "failed"
  | "cancelled";

export type MessageRole = "operator" | "agent" | "runtime";

export interface SessionMessage {
  seq: number;
  at_ms: number;
  role: MessageRole;
  text: string;
}

export type ApprovalDecision = "approved" | "denied";

export interface ApprovalRequest {
  id: string;
  requested_at_ms: number;
  run_id: string;
  node_id: string;
  node_type: string;
  capability: string;
  permissions: string[];
  reason: string;
  input: unknown;
  decision?: ApprovalDecision;
  decided_at_ms?: number;
  decided_by?: string;
}

export interface PlanPreview {
  at_ms: number;
  workflow: Workflow;
  valid: boolean;
  errors: number;
  warnings: number;
  diagnostics: Diagnostic[];
}

export interface AgentSession {
  id: string;
  goal: string;
  provider: string;
  status: SessionStatus;
  created_at_ms: number;
  updated_at_ms: number;
  messages: SessionMessage[];
  plan?: PlanPreview;
  approvals: ApprovalRequest[];
  run_id?: string;
  tokens_used: number;
}

export interface AgentSessionList {
  sessions: AgentSession[];
  pending_approvals: { session_id: string; approval: ApprovalRequest }[];
}

/* --- Audit --------------------------------------------------------------- */

export type AuditCategory =
  | "run_started"
  | "run_finished"
  | "node_started"
  | "node_finished"
  | "node_failed"
  | "capability_evaluated"
  | "approval"
  | "log";

/**
 * One audit record.
 *
 * The event stream is for live observation and a slow subscriber may fall
 * behind; these records are the durable account of what the runtime allowed,
 * refused and recorded.
 */
export interface AuditRecord {
  seq: number;
  timestamp_ms: number;
  run_id: string;
  category: AuditCategory;
  node_id?: string;
  node_type?: string;
  capability?: string;
  decision?: string;
  message: string;
  detail: unknown;
}
