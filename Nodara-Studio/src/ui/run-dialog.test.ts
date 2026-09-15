import { beforeEach, describe, expect, it } from "vitest";

import { emptyWorkflow } from "../model/workflow";
import { RunDialog } from "./run-dialog";

function dialogHarness() {
  const root = document.createElement("dialog");
  Object.defineProperty(root, "showModal", { configurable: true, value: () => undefined });
  Object.defineProperty(root, "close", { configurable: true, value: () => undefined });
  document.body.appendChild(root);
  const workflow = emptyWorkflow();
  workflow.variables.name = { value: "World" };
  workflow.variables.count = { value: 2 };
  workflow.variables.secret = { value: "stored", secret: true };
  const overrides = new Map<string, unknown>();
  let runs = 0;
  const dialog = new RunDialog(root, workflow, {
    onRun: () => { runs += 1; },
    getOverride: (name) => overrides.get(name),
    setOverride: (name, value) => overrides.set(name, value),
    clearOverride: (name) => overrides.delete(name),
  });
  return { root, workflow, overrides, dialog, runs: () => runs };
}

describe("run variables dialog", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  it("edits and clears session-only overrides", () => {
    const { root, overrides, dialog } = dialogHarness();
    dialog.open();

    const name = root.querySelector<HTMLInputElement>('input[data-variable-name="name"]')!;
    name.value = "Codex";
    name.dispatchEvent(new Event("input"));
    expect(overrides.get("name")).toBe("Codex");
    expect(root.textContent).toContain("Override set");

    const card = name.closest(".run-variable")!;
    card.querySelector<HTMLButtonElement>("button.btn--small")!.click();
    expect(overrides.has("name")).toBe(false);
    expect(name.value).toBe("World");
  });

  it("masks secret defaults and accepts a temporary secret", () => {
    const { root, overrides, dialog } = dialogHarness();
    dialog.open();

    const secret = root.querySelector<HTMLInputElement>('input[data-variable-name="secret"]')!;
    expect(secret.type).toBe("password");
    expect(secret.value).toBe("");
    expect(root.textContent).toContain("Stored secret");

    secret.value = "temporary";
    secret.dispatchEvent(new Event("input"));
    expect(overrides.get("secret")).toBe("temporary");
  });

  it("blocks Run until JSON variables are valid", () => {
    const { root, workflow, overrides, dialog, runs } = dialogHarness();
    workflow.variables.payload = { value: { a: 1 } };
    dialog.open();

    const payload = root.querySelector<HTMLTextAreaElement>('textarea[data-variable-name="payload"]')!;
    payload.value = "{";
    payload.dispatchEvent(new Event("input"));
    const run = root.querySelector<HTMLButtonElement>(".modal__footer .btn--primary")!;
    expect(run.disabled).toBe(true);

    payload.value = '{"b":2}';
    payload.dispatchEvent(new Event("input"));
    expect(run.disabled).toBe(false);
    expect(overrides.get("payload")).toEqual({ b: 2 });

    run.click();
    expect(runs()).toBe(1);
  });

  it("handles workflows without variables", () => {
    const root = document.createElement("dialog");
    Object.defineProperty(root, "showModal", { configurable: true, value: () => undefined });
    document.body.appendChild(root);
    const dialog = new RunDialog(root, emptyWorkflow(), {
      onRun: () => undefined,
      getOverride: () => undefined,
      setOverride: () => undefined,
      clearOverride: () => undefined,
    });
    dialog.open();
    expect(root.textContent).toContain("no variables");
  });
});
