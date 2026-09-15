import { beforeEach, describe, expect, it } from "vitest";

import { RunPanel } from "./run-panel";
import { RunSnapshot } from "../runtime/types";

function run(
  id: string,
  status: RunSnapshot["status"],
  overrides: Partial<RunSnapshot> = {},
): RunSnapshot {
  return {
    id,
    workflow_id: "workflow.test",
    status,
    started_at_ms: 1,
    nodes_executed: 3,
    variables: {},
    event_count: 3,
    artifact_count: 0,
    ...overrides,
  };
}

describe("run history panel", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  it("lists runs and opens the selected event stream", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const opened: string[] = [];
    const panel = new RunPanel(host, { onOpenRun: (id) => opened.push(id) });
    panel.setRuns([run("12345678-aaaa", "completed"), run("87654321-bbbb", "failed")]);

    expect(host.textContent).toContain("completed");
    expect(host.textContent).toContain("failed");
    host.querySelector<HTMLButtonElement>("button")!.click();
    expect(opened).toEqual(["12345678-aaaa"]);
  });

  it("shows accurate timing, counters and duration", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const panel = new RunPanel(host, { onOpenRun: () => undefined });
    panel.setRuns([
      run("12345678-aaaa", "completed", {
        started_at_ms: 1_000,
        finished_at_ms: 5_000,
        nodes_executed: 4,
        event_count: 12,
        artifact_count: 2,
      }),
    ]);

    const row = host.querySelector<HTMLTableRowElement>('[data-run-id="12345678-aaaa"]')!;
    const cells = [...row.querySelectorAll<HTMLTableCellElement>("td")];
    expect(cells[3].textContent).toBe("4");
    expect(cells[4].textContent).toBe("12");
    expect(cells[5].textContent).toBe("2");
    expect(cells[6].textContent).toBe(new Date(1_000).toLocaleString());
    expect(cells[7].textContent).toBe(new Date(5_000).toLocaleString());
    expect(cells[8].textContent).toBe("4.0 s");
  });

  it("filters by text and status", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const panel = new RunPanel(host, { onOpenRun: () => undefined });
    panel.setRuns([
      run("12345678-aaaa", "completed", { workflow_id: "workflow.alpha" }),
      run("87654321-bbbb", "failed", { workflow_id: "workflow.beta" }),
    ]);

    panel.filter("beta");
    expect(host.querySelectorAll("tbody tr")).toHaveLength(1);
    expect(host.textContent).toContain("workflow.beta");

    panel.filter("", "completed");
    expect(host.querySelectorAll("tbody tr")).toHaveLength(1);
    expect(host.textContent).toContain("workflow.alpha");

    panel.filter("does-not-exist");
    expect(host.textContent).toContain("No runs match");
  });

  it("shows an empty state", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    new RunPanel(host, { onOpenRun: () => undefined }).setRuns([]);
    expect(host.textContent).toContain("No runs yet");
  });
});
