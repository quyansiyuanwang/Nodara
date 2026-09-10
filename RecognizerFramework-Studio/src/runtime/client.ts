/**
 * The only place that knows how to talk to the runtime.
 *
 * Everything above this module works with the typed DTOs, which means a change
 * to the HTTP surface is a change to one file.
 */

import {
  AgentSession,
  AgentSessionList,
  ApprovalDecision,
  ApiErrorBody,
  EventEnvelope,
  JsonSchema,
  NodeDescriptor,
  PluginListResponse,
  RunSnapshot,
  ValidationReport,
  Workflow,
} from "./types";

export class RuntimeError extends Error {
  readonly status: number;
  readonly code: string;
  readonly detail: unknown;

  constructor(status: number, body: ApiErrorBody) {
    super(body.message);
    this.name = "RuntimeError";
    this.status = status;
    this.code = body.code;
    this.detail = body.detail;
  }
}

export interface RuntimeHealth {
  status: string;
  node_types: number;
  plugins: number;
  runs: number;
}

export interface ValidationOptions {
  reject_cycles?: boolean;
  require_start?: boolean;
  require_end?: boolean;
  warn_unreachable?: boolean;
  warn_dead_end?: boolean;
  check_variable_references?: boolean;
}

export interface EventStreamHandlers {
  onEvent: (envelope: EventEnvelope) => void;
  onError?: (error: Event) => void;
  onClose?: () => void;
}

export class RuntimeClient {
  constructor(private readonly baseUrl = "") {}

  private url(path: string): string {
    return `${this.baseUrl}/api/v1${path}`;
  }

  private async request<T>(path: string, init?: RequestInit): Promise<T> {
    const response = await fetch(this.url(path), {
      headers: { "content-type": "application/json" },
      ...init,
    });
    const text = await response.text();
    const payload = text ? JSON.parse(text) : null;
    if (!response.ok) {
      const body: ApiErrorBody = payload ?? {
        code: `HTTP_${response.status}`,
        message: response.statusText || "request failed",
      };
      throw new RuntimeError(response.status, body);
    }
    return payload as T;
  }

  health(): Promise<RuntimeHealth> {
    return this.request<RuntimeHealth>("/health");
  }

  plugins(): Promise<PluginListResponse> {
    return this.request<PluginListResponse>("/plugins");
  }

  async nodeTypes(): Promise<NodeDescriptor[]> {
    const payload = await this.request<{ node_types: NodeDescriptor[] }>("/node-types");
    return payload.node_types;
  }

  validate(workflow: Workflow, options?: ValidationOptions): Promise<ValidationReport> {
    return this.request<ValidationReport>("/workflows/validate", {
      method: "POST",
      body: JSON.stringify({ workflow, options }),
    });
  }

  createRun(workflow: Workflow, variables: Record<string, unknown> = {}): Promise<RunSnapshot> {
    return this.request<RunSnapshot>("/runs", {
      method: "POST",
      body: JSON.stringify({ workflow, variables }),
    });
  }

  listRuns(): Promise<RunSnapshot[]> {
    return this.request<RunSnapshot[]>("/runs");
  }

  getRun(runId: string): Promise<RunSnapshot> {
    return this.request<RunSnapshot>(`/runs/${encodeURIComponent(runId)}`);
  }

  pause(runId: string): Promise<RunSnapshot> {
    return this.post(`/runs/${encodeURIComponent(runId)}/pause`);
  }

  resume(runId: string): Promise<RunSnapshot> {
    return this.post(`/runs/${encodeURIComponent(runId)}/resume`);
  }

  step(runId: string): Promise<RunSnapshot> {
    return this.post(`/runs/${encodeURIComponent(runId)}/step`);
  }

  cancel(runId: string): Promise<RunSnapshot> {
    return this.post(`/runs/${encodeURIComponent(runId)}/cancel`);
  }

  /** Every agent session, plus the approvals waiting on an operator. */
  agentSessions(): Promise<AgentSessionList> {
    return this.request<AgentSessionList>("/agent/sessions");
  }

  /**
   * Answer an approval that is blocking a run.
   *
   * The run is genuinely parked until this returns: the runtime's approval
   * handler is blocking the run thread on the decision, not merely recording it.
   */
  decideApproval(
    sessionId: string,
    approvalId: string,
    decision: ApprovalDecision,
    decidedBy = "studio",
  ): Promise<AgentSession> {
    return this.post(
      `/agent/sessions/${encodeURIComponent(sessionId)}/approvals/${encodeURIComponent(approvalId)}`,
      { decision, decided_by: decidedBy },
    );
  }

  private post<T>(path: string, body: unknown = {}): Promise<T> {
    return this.request<T>(path, { method: "POST", body: JSON.stringify(body) });
  }

  /**
   * Subscribe to a run's event stream.
   *
   * The server replays every event recorded so far before streaming live ones,
   * so a late or reconnecting client never misses the start of a run. The
   * returned function closes the socket.
   */
  streamRunEvents(runId: string, handlers: EventStreamHandlers): () => void {
    const scheme = window.location.protocol === "https:" ? "wss" : "ws";
    const origin = this.baseUrl || `${scheme}://${window.location.host}`;
    const socket = new WebSocket(
      `${origin}/api/v1/runs/${encodeURIComponent(runId)}/events`,
    );

    socket.addEventListener("message", (message) => {
      try {
        handlers.onEvent(JSON.parse(message.data) as EventEnvelope);
      } catch {
        // A malformed frame must never take the viewer down.
      }
    });
    socket.addEventListener("error", (error) => handlers.onError?.(error));
    socket.addEventListener("close", () => handlers.onClose?.());

    return () => socket.close();
  }
}

/** Render a JSON Schema fragment for a single configuration key. */
export function schemaPlaceholder(schema: JsonSchema): string {
  if (schema.description) return schema.description;
  if (schema.enum) return schema.enum.map(String).join(" | ");
  if (schema.default !== undefined) return String(schema.default);
  return schema.type === undefined ? "any" : String(schema.type);
}
