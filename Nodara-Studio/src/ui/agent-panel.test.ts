import { describe, expect, it } from "vitest";

import { AgentPanel } from "./agent-panel";
import type { AgentPanelHandlers } from "./agent-panel";
import type { AgentSessionList } from "../runtime/types";

const handlers: AgentPanelHandlers = {
  onSubmit: async () => undefined,
  onStopGeneration: async () => false,
  onCredentialGet: async () => null,
  onCredentialSet: async () => undefined,
  onCredentialDelete: async () => undefined,
  getCurrentWorkflow: () => ({ schema_version: "2.1", id: "wf", metadata: { name: "wf", tags: [] }, nodes: [], edges: [], variables: {} }),
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

  it("filters the visible session list locally", () => {
    const root = document.createElement("div");
    const panel = new AgentPanel(root, handlers);
    const list = sessionList("capture desktop");
    list.sessions.push({
      ...list.sessions[0],
      id: "session-2",
      goal: "write a report",
    });
    panel.setSessions(list);

    const search = root.querySelector<HTMLInputElement>(".agent-search")!;
    search.value = "report";
    search.dispatchEvent(new Event("input", { bubbles: true }));

    const sessions = root.querySelectorAll(".session");
    expect(sessions).toHaveLength(1);
    expect(sessions[0].textContent).toContain("write a report");
  });

  it("keeps button focus and scroll position after a session refresh", () => {
    const root = document.createElement("div");
    document.body.appendChild(root);
    const panel = new AgentPanel(root, handlers);
    panel.setSessions(sessionList("first"));

    const main = root.querySelector<HTMLElement>(".agent-main")!;
    main.scrollTop = 120;
    const newChat = root.querySelector<HTMLButtonElement>('[data-agent-focus="new-chat"]')!;
    newChat.focus();

    panel.setSessions(sessionList("second"));

    const restored = root.querySelector<HTMLButtonElement>('[data-agent-focus="new-chat"]')!;
    expect(document.activeElement).toBe(restored);
    expect(root.querySelector<HTMLElement>(".agent-main")!.scrollTop).toBe(120);
  });

  it("renders streamed model deltas and plan phases", async () => {
    const root = document.createElement("div");
    document.body.appendChild(root);
    const streamHandlers: AgentPanelHandlers = {
      ...handlers,
      onSubmit: async (_request, _turnId, onEvent) => {
        onEvent({ type: "phase_changed", phase: "model_call", message: "Planning" });
        onEvent({ type: "model_delta", text: "{\"schema_version\":\"2.1\"}" });
      },
    };
    new AgentPanel(root, streamHandlers);
    const composer = root.querySelector<HTMLTextAreaElement>('[data-agent-focus="composer"]')!;
    composer.value = "make a workflow";
    composer.dispatchEvent(new Event("input", { bubbles: true }));
    root.querySelector<HTMLFormElement>(".agent-composer")!.dispatchEvent(
      new Event("submit", { bubbles: true, cancelable: true }),
    );
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(root.querySelector(".agent-stream")?.textContent).toContain("schema_version");
  });

  it("opens and persists the full workspace", () => {
    const root = document.createElement("div");
    document.body.appendChild(root);
    new AgentPanel(root, handlers);
    root.querySelector<HTMLButtonElement>('[data-agent-focus="workspace-toggle"]')!.click();
    expect(root.classList.contains("agent--workspace")).toBe(true);
    expect(localStorage.getItem("nodara.agent.settings.v1")).toContain('"workspaceOpen":true');
  });

  it("keeps provider input focus and caret after a session refresh", () => {
    const root = document.createElement("div");
    document.body.appendChild(root);
    const panel = new AgentPanel(root, handlers);
    panel.setSessions(sessionList("first"));

    const provider = root.querySelector<HTMLDetailsElement>(".agent-provider")!;
    provider.open = true;
    const endpoint = root.querySelector<HTMLInputElement>(
      '[data-agent-focus="provider.endpoint"]',
    )!;
    endpoint.value = "https://example.test/v1/chat/completions";
    endpoint.dispatchEvent(new Event("input", { bubbles: true }));
    endpoint.focus();
    endpoint.setSelectionRange(endpoint.value.length, endpoint.value.length);

    panel.setSessions(sessionList("second"));

    const restored = root.querySelector<HTMLInputElement>(
      '[data-agent-focus="provider.endpoint"]',
    )!;
    expect(document.activeElement).toBe(restored);
    expect(restored.value).toBe("https://example.test/v1/chat/completions");
    expect(restored.selectionStart).toBe(restored.value.length);
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
