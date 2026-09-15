import { localizeRunStatus, t } from "../i18n";
import { RunSnapshot } from "../runtime/types";

export interface RunPanelHandlers {
  onOpenRun: (runId: string) => void;
}

export class RunPanel {
  constructor(
    private readonly root: HTMLElement,
    private readonly handlers: RunPanelHandlers,
  ) {}

  setRuns(runs: RunSnapshot[]): void {
    this.root.replaceChildren();
    if (runs.length === 0) {
      const empty = document.createElement("p");
      empty.className = "muted";
      empty.textContent = t("runs.empty");
      this.root.appendChild(empty);
      return;
    }

    const table = document.createElement("table");
    table.className = "runs__table";
    const head = document.createElement("thead");
    const header = document.createElement("tr");
    for (const key of ["runs.id", "runs.workflow", "runs.status", "runs.nodes", "runs.started", "runs.open"]) {
      const cell = document.createElement("th");
      cell.textContent = t(key);
      header.appendChild(cell);
    }
    head.appendChild(header);
    table.appendChild(head);

    const body = document.createElement("tbody");
    for (const run of runs) {
      const row = document.createElement("tr");
      row.className = `run-row run-row--${run.status}`;
      const values = [
        run.id.slice(0, 8),
        run.workflow_id,
        localizeRunStatus(run.status),
        String(run.nodes_executed),
        new Date(run.finished_at_ms ?? run.started_at_ms).toLocaleString(),
      ];
      for (const value of values) {
        const cell = document.createElement("td");
        cell.textContent = value;
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
