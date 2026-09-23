import { beforeEach, describe, expect, it, vi } from "vitest";

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
  onObserveScreen: async () => undefined,
  listArtifacts: async () => [],
  artifactUrl: (runId, artifactId) => `/runs/${runId}/artifacts/${artifactId}`,
};

function sessionList(goal: string, runId?: string): AgentSessionList {
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
        run_id: runId,
        tokens_used: 0,
      },
    ],
    pending_approvals: [],
  };
}

function observeButton(root: HTMLElement): HTMLButtonElement {
  return root.querySelector<HTMLButtonElement>('[data-agent-focus="observe-screen"]')!;
}

beforeEach(() => {
  localStorage.clear();
  document.body.replaceChildren();
});

describe("AgentPanel settings", () => {
  it("does not rerender an unchanged polling response", () => {
    const root = document.createElement("div");
    const panel = new AgentPanel(root, handlers);
    expect(root.querySelector(".agent-shell")).not.toBeNull();

    panel.setSessions(sessionList("first"));

    const chip = root.querySelector(".agent-provider-chip");
    expect(chip).not.toBeNull();

    panel.setSessions(sessionList("first"));

    expect(root.querySelector(".agent-provider-chip")).toBe(chip);
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

  it("keeps provider input focus and caret while sessions refresh", () => {
    const root = document.createElement("div");
    document.body.appendChild(root);
    const panel = new AgentPanel(root, handlers);
    panel.setSessions(sessionList("first"));
    panel.openSettings();

    const endpoint = document.querySelector<HTMLInputElement>(
      '[data-agent-focus="provider.endpoint"]',
    )!;
    endpoint.value = "https://example.test/v1/chat/completions";
    endpoint.dispatchEvent(new Event("input", { bubbles: true }));
    endpoint.focus();
    endpoint.setSelectionRange(endpoint.value.length, endpoint.value.length);

    panel.setSessions(sessionList("second"));

    expect(document.activeElement).toBe(endpoint);
    expect(endpoint.value).toBe("https://example.test/v1/chat/completions");
    expect(endpoint.selectionStart).toBe(endpoint.value.length);
  });

  it("keeps the settings dialog open when session data changes", () => {
    const root = document.createElement("div");
    document.body.appendChild(root);
    const panel = new AgentPanel(root, handlers);
    panel.setSessions(sessionList("first"));
    panel.openSettings();
    const dialog = document.getElementById("agent-settings")!;

    panel.openSettings();
    panel.setSessions(sessionList("second"));

    expect(document.getElementById("agent-settings")).toBe(dialog);
    expect(dialog.hasAttribute("open")).toBe(true);
  });

  it("closes the settings dialog before collapsing the workspace", async () => {
    const root = document.createElement("div");
    document.body.appendChild(root);
    const panel = new AgentPanel(root, handlers);
    panel.setSessions(sessionList("first"));

    root.querySelector<HTMLButtonElement>('[data-agent-focus="workspace-toggle"]')!.click();
    expect(root.classList.contains("agent--workspace")).toBe(true);

    panel.openSettings();
    const dialog = document.getElementById("agent-settings")!;
    dialog.querySelector("input")!.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
    );
    await new Promise((resolve) => setTimeout(resolve, 0));

    // The dialog closed, and the Escape did not also collapse the workspace.
    expect(dialog.hasAttribute("open")).toBe(false);
    expect(root.classList.contains("agent--workspace")).toBe(true);

    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(root.classList.contains("agent--workspace")).toBe(false);
  });

  it("offers the prompt templates it ships with, marked as built-in", () => {
    const root = document.createElement("div");
    document.body.appendChild(root);
    new AgentPanel(root, handlers);

    const options = [...root.querySelectorAll<HTMLOptionElement>(
      '[data-agent-focus="prompt-template"] option',
    )];
    const builtin = options.filter((option) => option.textContent?.includes("built-in"));
    expect(builtin.length).toBeGreaterThanOrEqual(3);
    expect(builtin[0].value.startsWith("builtin.")).toBe(true);
  });

  it("explains what the selected execution mode will do", () => {
    const root = document.createElement("div");
    document.body.appendChild(root);
    new AgentPanel(root, handlers);
    const hint = root.querySelector(".agent-mode-hint")!;

    // The shared default is partial approval, and the row says so.
    expect(hint.textContent).toContain("waits for your approval");

    const mode = root.querySelector<HTMLSelectElement>('[data-agent-focus="execution-mode"]')!;
    mode.value = "forbidden";
    mode.dispatchEvent(new Event("change", { bubbles: true }));
    expect(root.querySelector(".agent-mode-hint")!.textContent).toContain("Plans only");
  });

  it("observes the screen for the selected session only", async () => {
    const root = document.createElement("div");
    document.body.appendChild(root);
    const onObserveScreen = vi.fn(async () => undefined);
    const panel = new AgentPanel(root, { ...handlers, onObserveScreen });

    // Nothing to observe before a session exists, and the button says why.
    expect(observeButton(root).disabled).toBe(true);

    panel.setSessions(sessionList("capture the screen"));
    expect(observeButton(root).disabled).toBe(false);
    observeButton(root).click();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(onObserveScreen).toHaveBeenCalledWith("session-1", "partial");
  });

  it("shows the screenshots the next turn will receive", async () => {
    const root = document.createElement("div");
    document.body.appendChild(root);
    const listArtifacts = vi.fn(async () => [
      { id: "a1", name: "desktop.png", content_type: "image/png", size: 10 },
      { id: "a2", name: "log.txt", content_type: "text/plain", size: 4 },
    ]);
    const panel = new AgentPanel(root, { ...handlers, listArtifacts });
    panel.setSessions(sessionList("capture", "run-7"));
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(listArtifacts).toHaveBeenCalledWith("run-7");
    const evidence = root.querySelector(".agent-evidence")!;
    expect(evidence.textContent).toContain("1 screenshot");
    expect(evidence.querySelectorAll(".agent-evidence__thumb")).toHaveLength(1);
    expect(evidence.querySelector<HTMLImageElement>(".agent-evidence__thumb")!.src)
      .toContain("/runs/run-7/artifacts/a1");
  });
});
