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
  private query = "";
  private follow = true;

  constructor(
    private readonly root: HTMLElement,
    private readonly canvas: Canvas,
    private readonly artifactUrl?: (runId: string, artifactId: string) => string,
    private readonly countElement?: HTMLElement,
  ) {}

  clear(): void {
    this.root.replaceChildren();
    this.updateCount();
  }

  /** Filter rendered events by type, node, message or output metadata. */
  filter(query: string): void {
    this.query = query.trim().toLowerCase();
    for (const entry of this.root.querySelectorAll<HTMLElement>(".event-entry")) {
      entry.hidden = !this.matches(entry.dataset.search ?? "");
    }
    this.updateCount();
  }

  /** Keep the newest visible event in view as new events arrive. */
  setFollow(follow: boolean): void {
    this.follow = follow;
    if (follow) this.root.scrollTop = this.root.scrollHeight;
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
    const entry = document.createElement("div");
    entry.className = "event-entry";
    entry.dataset.search = `${event.type} ${describe(envelope)} ${JSON.stringify(event)}`.toLowerCase();
    entry.hidden = !this.matches(entry.dataset.search);
    entry.appendChild(row);
    this.appendArtifactPreviews(entry, envelope);
    this.appendCommandOutput(entry, envelope);
    this.root.appendChild(entry);
    while (this.root.querySelectorAll(".event-entry").length > MAX_ROWS) {
      this.root.firstElementChild?.remove();
    }
    this.updateCount();
    if (this.follow) this.root.scrollTop = this.root.scrollHeight;

    this.decorate(event);
  }

  private matches(search: string): boolean {
    return this.query === "" || search.includes(this.query);
  }

  private updateCount(): void {
    if (!this.countElement) return;
    const entries = this.root.querySelectorAll<HTMLElement>(".event-entry");
    const visible = [...entries].filter((entry) => !entry.hidden).length;
    this.countElement.textContent = `${visible}/${entries.length}`;
  }

  /** Render image artifacts attached to successful node events. */
  private appendArtifactPreviews(entry: HTMLElement, envelope: EventEnvelope): void {
    if (!this.artifactUrl) return;
    for (const [port, artifact] of artifactsIn(envelope.event)) {
      const url = this.artifactUrl(envelope.run_id, artifact.id);
      const preview = document.createElement("figure");
      preview.className = "event-artifact";
      const image = document.createElement("img");
      image.className = "event-artifact__image";
      image.src = url;
      image.alt = t("artifact.previewAlt", { name: artifact.name });
      image.loading = "lazy";
      image.decoding = "async";
      const failure = document.createElement("div");
      failure.className = "event-artifact__error";
      failure.textContent = t("artifact.previewUnavailable");
      failure.hidden = true;
      image.addEventListener("error", () => {
        image.hidden = true;
        failure.hidden = false;
      });
      const media = document.createElement("div");
      media.className = "event-artifact__media";
      media.append(image, failure);
      const caption = document.createElement("figcaption");
      caption.className = "event-artifact__caption";
      const details = document.createElement("span");
      details.textContent = t("artifact.previewDetails", {
        port,
        type: artifact.content_type,
        size: artifact.size,
      });
      const open = document.createElement("a");
      open.href = url;
      open.target = "_blank";
      open.rel = "noreferrer";
      open.textContent = t("actions.open");
      caption.append(details, open);
      preview.append(media, caption);
      entry.appendChild(preview);
    }
  }

  /** Show external-command output directly in the event stream. */
  private appendCommandOutput(entry: HTMLElement, envelope: EventEnvelope): void {
    if (envelope.event.type !== "node_finished") return;
    const outputs = envelope.event.outputs;
    const commandLike =
      "exit_code" in outputs || "stderr" in outputs || "success" in outputs;
    if (!commandLike) return;

    const details = document.createElement("details");
    details.className = "event-command";
    details.open = true;
    const summary = document.createElement("summary");
    summary.textContent = t("command.output");
    const grid = document.createElement("div");
    grid.className = "event-command__grid";
    grid.append(
      outputBlock(t("command.stdout"), outputText(outputs.out)),
      outputBlock(t("command.stderr"), outputText(outputs.stderr)),
    );
    const meta = document.createElement("p");
    meta.className = "event-command__meta";
    meta.textContent = t("command.exitCode", {
      code: String(outputs.exit_code ?? "–"),
      pid: String(outputs.pid ?? "–"),
    });
    details.append(summary, grid, meta);
    entry.appendChild(details);
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

interface ArtifactPreviewMeta {
  id: string;
  name: string;
  content_type: string;
  size: number;
}

function artifactMeta(value: unknown): ArtifactPreviewMeta | null {
  if (!value || typeof value !== "object") return null;
  const record = value as Record<string, unknown>;
  if (
    typeof record.id !== "string" ||
    typeof record.content_type !== "string" ||
    typeof record.size !== "number"
  ) {
    return null;
  }
  return {
    id: record.id,
    name: typeof record.name === "string" ? record.name : record.id,
    content_type: record.content_type,
    size: record.size,
  };
}

/** Image artifacts directly attached to node output or logged as JSON. */
function artifactsIn(event: ExecutionEvent): Array<[string, ArtifactPreviewMeta]> {
  const values: Array<[string, unknown]> = [];
  if (event.type === "node_finished") {
    values.push(...Object.entries(event.outputs));
  } else if (event.type === "log") {
    const message = parseJson(event.message);
    if (message !== null) values.push(["log", message]);
  }
  const artifacts: Array<[string, ArtifactPreviewMeta]> = [];
  for (const [port, value] of values) {
    const artifact = artifactMeta(value);
    if (artifact?.content_type.startsWith("image/")) artifacts.push([port, artifact]);
  }
  return artifacts;
}

function parseJson(value: string): unknown {
  const trimmed = value.trim();
  if (!trimmed.startsWith("{") && !trimmed.startsWith("[")) return null;
  try {
    return JSON.parse(trimmed) as unknown;
  } catch {
    return null;
  }
}

function outputText(value: unknown): string {
  if (value === undefined || value === null || value === "") return t("command.empty");
  return typeof value === "string" ? value : JSON.stringify(value, null, 2);
}

function outputBlock(label: string, text: string): HTMLElement {
  const block = document.createElement("div");
  block.className = "event-command__block";
  const heading = document.createElement("strong");
  heading.textContent = label;
  const pre = document.createElement("pre");
  pre.textContent = text;
  block.append(heading, pre);
  return block;
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
