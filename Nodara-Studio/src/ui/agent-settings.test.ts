/**
 * The Agent configuration dialog.
 *
 * These cover the behaviour that used to be impossible in the old inline form:
 * profile presets, credential state, inline validation that never writes a bad
 * value through, and prompt templates that exist without the operator writing
 * one.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";

import { defaultAgentSettings, findTemplate, loadAgentSettings } from "../model/agent-settings";
import { setLocale } from "../i18n";
import { AgentSettingsDialog } from "./agent-settings";
import type { AgentSettingsDialogHandlers } from "./agent-settings";

function makeHandlers(overrides: Partial<AgentSettingsDialogHandlers> = {}): AgentSettingsDialogHandlers {
  return {
    onChange: vi.fn(),
    onCredentialGet: vi.fn(async () => null),
    onCredentialSet: vi.fn(async () => undefined),
    onCredentialDelete: vi.fn(async () => undefined),
    ...overrides,
  };
}

function field(selector: string): HTMLInputElement {
  const node = document.querySelector<HTMLInputElement>(selector);
  if (!node) throw new Error(`missing ${selector}`);
  return node;
}

function type(input: HTMLInputElement, value: string): void {
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

describe("AgentSettingsDialog", () => {
  beforeEach(() => {
    document.body.replaceChildren();
    setLocale("en");
  });

  it("renders the provider fields and every built-in template", () => {
    const settings = defaultAgentSettings();
    const dialog = new AgentSettingsDialog(makeHandlers());
    dialog.open(settings);

    expect(field('[data-agent-focus="provider.endpoint"]').value).toBe(settings.profiles[0].endpoint);
    expect(field('[data-agent-focus="provider.model"]').value).toBe(settings.profiles[0].model);
    expect(document.querySelector('[data-agent-focus="provider.timeoutMs"]')).not.toBeNull();
    expect(document.querySelectorAll(".agent-template--builtin").length).toBeGreaterThan(0);
    // Built-ins are described, never offered for removal.
    expect(
      document.querySelector('.agent-template--builtin [data-agent-focus^="template-remove"]'),
    ).toBeNull();
  });

  it("fills the endpoint and model from a provider preset", () => {
    const settings = defaultAgentSettings();
    const handlers = makeHandlers();
    const dialog = new AgentSettingsDialog(handlers);
    dialog.open(settings);

    const preset = document.querySelector<HTMLSelectElement>('[data-agent-focus="provider-preset"]')!;
    preset.value = "deepseek";
    preset.dispatchEvent(new Event("change", { bubbles: true }));
    document.querySelector<HTMLButtonElement>('[data-agent-focus="provider-preset-apply"]')!.click();

    expect(settings.profiles[0].endpoint).toContain("api.deepseek.com");
    expect(settings.profiles[0].model).toBe("deepseek-chat");
    expect(handlers.onChange).toHaveBeenCalled();
    expect(field('[data-agent-focus="provider.model"]').value).toBe("deepseek-chat");
  });

  it("reports an invalid endpoint without writing it through", () => {
    const settings = defaultAgentSettings();
    const original = settings.profiles[0].endpoint;
    const dialog = new AgentSettingsDialog(makeHandlers());
    dialog.open(settings);

    const endpoint = field('[data-agent-focus="provider.endpoint"]');
    type(endpoint, "not-a-url");

    expect(endpoint.getAttribute("aria-invalid")).toBe("true");
    expect(document.querySelector(".field__hint--error:not([hidden])")).not.toBeNull();
    expect(settings.profiles[0].endpoint).toBe(original);
  });

  it("rejects a timeout outside the allowed range", () => {
    const settings = defaultAgentSettings();
    const dialog = new AgentSettingsDialog(makeHandlers());
    dialog.open(settings);

    const timeout = field('[data-agent-focus="provider.timeoutMs"]');
    type(timeout, "5");
    expect(settings.profiles[0].timeoutMs).toBe(300_000);

    type(timeout, "60000");
    expect(settings.profiles[0].timeoutMs).toBe(60_000);
  });

  it("shows the credential state and saves a key", async () => {
    const settings = defaultAgentSettings();
    const onCredentialSet = vi.fn(async () => undefined);
    const dialog = new AgentSettingsDialog(
      makeHandlers({ onCredentialSet, onCredentialGet: async () => "stored-key" }),
    );
    dialog.open(settings);
    await dialog.refreshCredential();

    expect(document.querySelector('[data-agent-focus="provider.apiKeyState"]')!.textContent)
      .toContain("saved");
    expect(dialog.activeApiKey()).toBe("stored-key");

    const input = field('[data-agent-focus="provider.apiKey"]');
    input.value = "next-key";
    input.dispatchEvent(new Event("change", { bubbles: true }));
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(onCredentialSet).toHaveBeenCalledWith(settings.profiles[0].id, "next-key");
    expect(dialog.activeApiKey()).toBe("next-key");
  });

  it("offers a retry when the credential store cannot be read", async () => {
    const onCredentialGet = vi.fn(async () => {
      throw new Error("credential store unavailable");
    });
    const dialog = new AgentSettingsDialog(makeHandlers({ onCredentialGet }));
    dialog.open(defaultAgentSettings());
    await dialog.refreshCredential();

    expect(onCredentialGet).toHaveBeenCalled();
    const state = document.querySelector('[data-agent-focus="provider.apiKeyState"]')!;
    expect(state.textContent).toContain("Could not read");
    expect(document.querySelector('[data-agent-focus="provider-api-key-retry"]')).not.toBeNull();
  });

  it("requires confirmation before removing a profile and keeps the last one", () => {
    const settings = defaultAgentSettings();
    settings.profiles.push({
      id: "second",
      name: "Second",
      endpoint: "https://example.test/v1/chat/completions",
      model: "m",
      timeoutMs: 1_000,
    });
    const dialog = new AgentSettingsDialog(makeHandlers());
    dialog.open(settings);

    const remove = document.querySelector<HTMLButtonElement>(
      '[data-agent-focus="provider-remove-profile"]',
    )!;
    remove.click();
    expect(settings.profiles).toHaveLength(2);
    expect(remove.textContent).toContain("Confirm");
    remove.click();
    expect(settings.profiles).toHaveLength(1);
    expect(
      document.querySelector<HTMLButtonElement>('[data-agent-focus="provider-remove-profile"]')!.disabled,
    ).toBe(true);
  });

  it("adds, edits and removes an operator template", () => {
    const settings = defaultAgentSettings();
    const dialog = new AgentSettingsDialog(makeHandlers());
    dialog.open(settings);

    document.querySelector<HTMLButtonElement>('[data-agent-focus="template-add"]')!.click();
    expect(settings.templates).toHaveLength(1);
    const id = settings.templates[0].id;

    const instructions = document.querySelector<HTMLTextAreaElement>(
      `[data-agent-focus="template.instructions.${id}"]`,
    )!;
    instructions.value = "keep it simple";
    instructions.dispatchEvent(new Event("input", { bubbles: true }));
    expect(settings.templates[0].instructions).toBe("keep it simple");

    const remove = document.querySelector<HTMLButtonElement>(
      `[data-agent-focus="template-remove.${id}"]`,
    )!;
    remove.click();
    remove.click();
    expect(settings.templates).toHaveLength(0);
  });

  it("counts the extra instructions sent with a turn", () => {
    const settings = defaultAgentSettings();
    const dialog = new AgentSettingsDialog(makeHandlers());
    dialog.open(settings);

    expect(document.querySelector(".agent-composition__note")!.textContent)
      .toContain("sent on its own");

    const extra = document.querySelector<HTMLTextAreaElement>(
      '[data-agent-focus="extra-instructions"]',
    )!;
    extra.value = "always log the result";
    extra.dispatchEvent(new Event("input", { bubbles: true }));

    expect(settings.extraInstructions).toBe("always log the result");
    expect(document.querySelector(".agent-composition__note")!.textContent)
      .toContain("21 character");
  });

  it("resolves a built-in template by id in the active language", () => {
    const settings = defaultAgentSettings();
    const english = findTemplate("builtin.observe-first", settings.templates);
    expect(english?.name).toBe("Observe first");
    expect(english?.instructions).toContain("capture");

    setLocale("zh-CN");
    expect(findTemplate("builtin.observe-first", settings.templates)?.name).toBe("观测优先");
  });

  it("closes on a backdrop click, on Escape inside it, and from the close button", () => {
    const dialog = new AgentSettingsDialog(makeHandlers());
    dialog.open(defaultAgentSettings());
    const root = document.getElementById("agent-settings")!;

    root.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(dialog.isOpen()).toBe(false);

    dialog.open(defaultAgentSettings());
    root.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(dialog.isOpen()).toBe(false);

    dialog.open(loadAgentSettings());
    expect(dialog.isOpen()).toBe(true);
    root.querySelector<HTMLButtonElement>(".modal__close")!.click();
    expect(dialog.isOpen()).toBe(false);
  });

  it("stays honest when the user agent closes the dialog itself", () => {
    const dialog = new AgentSettingsDialog(makeHandlers());
    dialog.open(defaultAgentSettings());
    const root = document.getElementById("agent-settings")!;
    root.removeAttribute("open");
    root.dispatchEvent(new Event("close"));

    expect(dialog.isOpen()).toBe(false);
  });
});
