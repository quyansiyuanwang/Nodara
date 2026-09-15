import { localizeRunStatus, t } from "../i18n";
import { RunSnapshot } from "../runtime/types";

export interface RunPanelHandlers {
  onOpenRun: (runId: string) => void;
}

export class RunPanel {
  private runs: RunSnapshot[] = [];
  private query = "";
  private status = "";

  constructor(
    private readonly root: HTMLElement,
    private readonly handlers: RunPanelHandlers,
  ) {}

  setRuns(runs: RunSnapshot[]): void {
    this.runs = [...runs].sort((a, b) => b.started_at_ms - a.started_at_ms);
    this.render();
  }

  filter(query: string, status = this.status): void {
    this.query = query.trim().toLowerCase();
    this.status = status;
    this.render();
  }

  private visibleRuns(): RunSnapshot[] {
    return this.runs.filter((run) => {
      if (this.status && run.status !== this.status) return false;
      if (!this.query) return true;
      return `${run.id} ${run.workflow_id} ${run.status}`.toLowerCase().includes(this.query);
    });
  }

  private render(): void {
    this.root.replaceChildren();
    const runs = this.visibleRuns();
    if (runs.length === 0) {
      const empty = document.createElement("p");
      empty.className = "muted";
      empty.textContent = this.runs.length === 0 ? t("runs.empty") : t("runs.noMatches");
      this.root.appendChild(empty);
      return;
    }

    const table = document.createElement("table");
    table.className = "runs__table";
    const head = document.createElement("thead");
    const header = document.createElement("tr");
    const columns = [
      "runs.id",
      "runs.workflow",
      "runs.status",
      "runs.nodes",
      "runs.events",
      "runs.artifacts",
      "runs.started",
      "runs.finished",
      "runs.duration",
      "runs.open",
    ];
    for (const key of columns) {
      const cell = document.createElement("th");
      cell.dataset.column = key.replace("runs.", "");
      cell.textContent = t(key);
      header.appendChild(cell);
    }
    head.appendChild(header);
    table.appendChild(head);

    const body = document.createElement("tbody");
    for (const run of runs) {
      const row = document.createElement("tr");
      row.className = `run-row run-row--${run.status}`;
      row.dataset.runId = run.id;
      const finished = run.finished_at_ms;
      const values = [
        run.id.slice(0, 8),
        run.workflow_id,
        localizeRunStatus(run.status),
        String(run.nodes_executed),
        String(run.event_count),
        String(run.artifact_count ?? 0),
        formatTime(run.started_at_ms),
        finished === undefined ? "—" : formatTime(finished),
        formatDuration((finished ?? Date.now()) - run.started_at_ms),
      ];
      for (const value of values) {
        const cell = document.createElement("td");
        cell.textContent = value;
        cell.dataset.value = value;
        row.appendChild(cell);
      }
      const action = document.createElement("td");
      const open = document.createElement("button");
      open.type = "button";
      open.className = "btn btn--small";
      open.textContent = t("runs.open");
      open.addEventListener("click", () => this.handlers.onOpenRun(run.id));
      action.appendChild(open);
      row.appendChild(action);
      body.appendChild(row);
    }
    table.appendChild(body);
    this.root.appendChild(table);
  }
}

function formatTime(value: number): string {
  return new Date(value).toLocaleString();
}

function formatDuration(value: number): string {
  const milliseconds = Math.max(0, value);
  if (milliseconds < 1000) return `${Math.round(milliseconds)} ms`;
  if (milliseconds < 60_000) return `${(milliseconds / 1000).toFixed(1)} s`;
  const totalSeconds = Math.floor(milliseconds / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}m ${String(seconds).padStart(2, "0")}s`;
}
