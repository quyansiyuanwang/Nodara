import { beforeEach, describe, expect, it } from "vitest";

import { RunPanel } from "./run-panel";
import { RunSnapshot } from "../runtime/types";

function run(id: string, status: RunSnapshot["status"]): RunSnapshot {
  return {
    id,
    workflow_id: "workflow.test",
    status,
    started_at_ms: 1,
    nodes_executed: 3,
    variables: {},
    event_count: 3,
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

  it("shows an empty state", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    new RunPanel(host, { onOpenRun: () => undefined }).setRuns([]);
    expect(host.textContent).toContain("No runs yet");
  });
});
