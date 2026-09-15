import { describe, expect, it } from "vitest";

import { AgentPanel } from "./agent-panel";
import type { AgentPanelHandlers } from "./agent-panel";
import type { AgentSessionList } from "../runtime/types";

const handlers: AgentPanelHandlers = {
  onSubmit: async () => undefined,
  onDecide: () => undefined,
  onLoadPlan: () => undefined,
  onOpenRun: () => undefined,
  onOpenAudit: () => undefined,
  onResumeRun: () => undefined,
  onValidatePlan: async () => ({ diagnostics: [] }),
  onRunPlan: async () => undefined,
};

function sessionList(goal: string): AgentSessionList {
  return {
    sessions: [
      {
        id: "session-1",
        goal,
        provider: "test",
        status: "completed",
        created_at_ms: 1,
        updated_at_ms: 2,
        messages: [],
        approvals: [],
        tokens_used: 0,
      },
    ],
    pending_approvals: [],
  };
}

describe("AgentPanel settings", () => {
  it("does not rerender an unchanged polling response", () => {
    const root = document.createElement("div");
    const panel = new AgentPanel(root, handlers);
    expect(root.querySelector(".agent-shell")).not.toBeNull();

    panel.setSessions(sessionList("first"));

    const provider = root.querySelector<HTMLDetailsElement>(".agent-provider");
    expect(provider).not.toBeNull();
    provider!.open = true;

    panel.setSessions(sessionList("first"));

    expect(root.querySelector(".agent-provider")).toBe(provider);
    expect(provider!.open).toBe(true);
  });

  it("keeps provider details open when session data changes", () => {
    const root = document.createElement("div");
    const panel = new AgentPanel(root, handlers);
    panel.setSessions(sessionList("first"));

    const provider = root.querySelector<HTMLDetailsElement>(".agent-provider")!;
    provider.open = true;

    panel.setSessions(sessionList("second"));

    expect(root.querySelector<HTMLDetailsElement>(".agent-provider")!.open).toBe(true);
  });
});
