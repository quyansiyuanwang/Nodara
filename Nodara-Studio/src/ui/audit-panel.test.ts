import { beforeEach, describe, expect, it } from "vitest";

import { AuditPanel } from "./audit-panel";
import { AuditRecord } from "../runtime/types";

function record(overrides: Partial<AuditRecord>): AuditRecord {
  return {
    seq: 0,
    timestamp_ms: 1_700_000_000_000,
    run_id: "run-12345678",
    category: "capability_evaluated",
    message: "",
    detail: null,
    ...overrides,
  };
}

describe("audit view", () => {
  let host: HTMLElement;

  beforeEach(() => {
    document.body.innerHTML = "";
    host = document.createElement("div");
    document.body.appendChild(host);
  });

  it("says so when there is nothing recorded", () => {
    new AuditPanel(host).setRecords([]);
    expect(host.textContent).toContain("No audit records yet");
  });

  it("renders the columns an operator reviews", () => {
    const panel = new AuditPanel(host);
    panel.setRecords([
      record({
        category: "capability_evaluated",
        node_id: "keyboard",
        capability: "windows.Input.Keyboard",
        decision: "require_approval",
        message: "capability `windows.Input.Keyboard` -> require_approval",
      }),
    ]);
    const headers = [...host.querySelectorAll("th")].map((cell) => cell.textContent);
    expect(headers).toEqual([
      "time",
      "run",
      "category",
      "node",
      "capability",
      "decision",
      "message",
    ]);

    const cells = [...host.querySelectorAll("td")].map((cell) => cell.textContent);
    expect(cells[1]).toBe("run-1234");
    expect(cells[2]).toBe("capability");
    expect(cells[3]).toBe("keyboard");
    expect(cells[4]).toBe("windows.Input.Keyboard");
    expect(cells[5]).toBe("require_approval");
  });

  it("highlights decisions and failures", () => {
    const panel = new AuditPanel(host);
    panel.setRecords([
      record({ seq: 0, decision: "allow", category: "capability_evaluated" }),
      record({ seq: 1, decision: "deny", category: "capability_evaluated" }),
      record({ seq: 2, category: "node_failed", node_id: "log", message: "boom" }),
    ]);
    expect(host.querySelector(".audit__decision--allow")).not.toBeNull();
    expect(host.querySelector(".audit__decision--deny")).not.toBeNull();
    expect(host.querySelector(".audit__row--node-failed")).not.toBeNull();
  });

  it("replaces the contents on refresh", () => {
    const panel = new AuditPanel(host);
    panel.setRecords([record({ message: "first" })]);
    panel.setRecords([record({ message: "second" })]);
    const rows = host.querySelectorAll("tbody tr");
    expect(rows).toHaveLength(1);
    expect(host.textContent).toContain("second");
    expect(host.textContent).not.toContain("first");
  });
});
