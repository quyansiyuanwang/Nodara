/**
 * The audit view.
 *
 * The plan's last migration step calls for an audit interface. This is it: the
 * durable record of every capability the runtime evaluated, every approval and
 * every node outcome — the same records the CLI writes and the runtime's policy
 * layer produces, so the two can never disagree.
 */

import { localizeAuditCategory, t } from "../i18n";
import { AuditRecord } from "../runtime/types";

export class AuditPanel {
  constructor(private readonly root: HTMLElement) {}

  setRecords(records: AuditRecord[]): void {
    this.root.replaceChildren();
    if (records.length === 0) {
      const empty = document.createElement("p");
      empty.className = "muted";
      empty.textContent = t("audit.empty");
      this.root.appendChild(empty);
      return;
    }

    const table = document.createElement("table");
    table.className = "audit__table";
    const head = document.createElement("thead");
    const headerKeys = [
      "audit.time",
      "audit.run",
      "audit.category",
      "audit.node",
      "audit.capability",
      "audit.decision",
      "audit.message",
    ];
    const headerRow = document.createElement("tr");
    for (const key of headerKeys) {
      const cell = document.createElement("th");
      cell.textContent = t(key);
      headerRow.appendChild(cell);
    }
    head.appendChild(headerRow);
    table.appendChild(head);

    const body = document.createElement("tbody");
    for (const record of records) {
      const row = document.createElement("tr");
      // CSS classes read better with hyphens, the wire format uses snake_case.
      row.className = `audit__row audit__row--${record.category.replace(/_/g, "-")}`;
      // The decision column is the one an operator scans for, so make it a
      // distinct cell rather than burying it in the message.
      const cells: (string | undefined)[] = [
        new Date(record.timestamp_ms).toLocaleTimeString(),
        record.run_id.slice(0, 8),
        localizeAuditCategory(record.category),
        record.node_id,
        record.capability,
        record.decision,
        record.message,
      ];
      for (const value of cells) {
        const cell = document.createElement("td");
        cell.textContent = value ?? "";
        if (value === record.decision && record.decision) {
          cell.className = `audit__decision audit__decision--${record.decision.replace(/_/g, "-")}`;
        }
        row.appendChild(cell);
      }
      body.appendChild(row);
    }
    table.appendChild(body);
    this.root.appendChild(table);
  }
}
