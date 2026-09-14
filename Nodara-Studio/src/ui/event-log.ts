/**
 * The runtime event viewer.
 *
 * It consumes the same WebSocket stream the CLI prints and the agent observes,
 * which is the point: one event contract, three consumers.
 */

import { t } from "../i18n";
import { Canvas } from "./canvas";
import { EventEnvelope, ExecutionEvent, RunStatus } from "../runtime/types";

const MAX_ROWS = 500;

const LABEL_KEYS: Record<string, string> = {
  run_started: "event.runStarted",
  node_started: "event.nodeStarted",
  node_progress: "event.progress",
  node_finished: "event.nodeFinished",
  node_failed: "event.nodeFailed",
  log: "event.log",
  run_paused: "event.paused",
  run_resumed: "event.resumed",
  run_cancelled: "event.cancelled",
  run_completed: "event.completed",
  run_failed: "event.failed",
  capability_decision: "event.policy",
};

export class EventLog {
  constructor(
    private readonly root: HTMLElement,
    private readonly canvas: Canvas,
  ) {}

  clear(): void {
    this.root.replaceChildren();
  }

  append(envelope: EventEnvelope): void {
    const event = envelope.event;
    const row = document.createElement("div");
    row.className = `event event--${event.type.replace(/_/g, "-")}`;

    const time = document.createElement("span");
    time.className = "event__time";
    time.textContent = new Date(envelope.timestamp_ms).toLocaleTimeString();

    const kind = document.createElement("span");
    kind.className = "event__kind";
    kind.textContent = LABEL_KEYS[event.type] ? t(LABEL_KEYS[event.type]) : event.type;

    const body = document.createElement("span");
    body.className = "event__body";
    body.textContent = describe(envelope);

    row.append(time, kind, body);
    this.root.appendChild(row);
    while (this.root.childElementCount > MAX_ROWS) {
      this.root.firstElementChild?.remove();
    }
    this.root.scrollTop = this.root.scrollHeight;

    this.decorate(event);
  }

  /** Reflect execution state on the canvas. */
  private decorate(event: ExecutionEvent): void {
    switch (event.type) {
      case "node_started":
        this.canvas.setActiveNode(event.node_id);
        this.canvas.setNodeState(event.node_id, "running");
        break;
      case "node_finished":
        this.canvas.setActiveNode(null);
        this.canvas.setNodeState(event.node_id, "done");
        break;
      case "node_failed":
        this.canvas.setActiveNode(null);
        this.canvas.setNodeState(event.node_id, "failed");
        break;
      default:
        break;
    }
  }

  setRunStatus(status: RunStatus | null): void {
    this.canvas.setStatus(status);
    if (status === null || status === "pending") {
      this.canvas.clearStates();
    }
  }
}

function describe(envelope: EventEnvelope): string {
  const event = envelope.event;
  switch (event.type) {
    case "run_started":
      return event.workflow_id;
    case "node_started":
      return `${event.node_id} (${event.node_type})`;
    case "node_progress":
      return `${event.node_id} ${event.progress !== undefined ? `${Math.round(event.progress * 100)}%` : ""} ${event.message ?? ""}`.trim();
    case "node_finished":
      return t("event.nodeFinishedBody", { node: event.node_id, duration: event.duration_ms });
    case "node_failed":
      return t("event.nodeFailedBody", { node: event.node_id, code: event.code, message: event.message });
    case "log":
      return `${event.level}: ${event.message}`;
    case "run_completed":
      return t("event.completedBody", { nodes: event.nodes_executed, duration: event.duration_ms });
    case "run_failed":
      return `[${event.code}] ${event.message}`;
    case "run_cancelled":
      return event.reason ?? t("event.cancelled");
    case "capability_decision":
      return `${event.capability}: ${event.decision}`;
    default:
      return "";
  }
}
