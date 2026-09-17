import { afterEach, describe, expect, it, vi } from "vitest";

import pageSource from "../../index.html?raw";

import {
  annotateToolbarShortcuts,
  bindingsFor,
  installShortcuts,
  isTypingTarget,
  resolveShortcut,
  SHORTCUTS,
  shortcutLabel,
  shortcutLabelFor,
  TOOLBAR_COMMANDS,
} from "./shortcuts";

function press(init: KeyboardEventInit): KeyboardEvent {
  return new KeyboardEvent("keydown", { cancelable: true, ...init });
}

describe("shortcut table", () => {
  it("binds Visual Studio's run keys to the run controls", () => {
    expect(resolveShortcut(press({ key: "F5" }), "windows", false)?.command).toBe("run");
    expect(resolveShortcut(press({ key: "F5", shiftKey: true }), "windows", false)?.command).toBe(
      "cancel",
    );
    expect(
      resolveShortcut(press({ key: "F5", ctrlKey: true }), "windows", false)?.command,
    ).toBe("runOptions");
    expect(
      resolveShortcut(press({ key: "F5", ctrlKey: true, shiftKey: true }), "windows", false)
        ?.command,
    ).toBe("restart");
    expect(resolveShortcut(press({ key: "F10" }), "windows", false)?.command).toBe("step");
  });

  it("does not fire a chord whose modifiers do not match exactly", () => {
    expect(resolveShortcut(press({ key: "F5", altKey: true }), "windows", false)).toBeNull();
    expect(resolveShortcut(press({ key: "z" }), "windows", false)).toBeNull();
    expect(resolveShortcut(press({ key: "q", ctrlKey: true }), "windows", false)).toBeNull();
  });

  it("uses the platform's command key", () => {
    expect(resolveShortcut(press({ key: "s", ctrlKey: true }), "windows", false)?.command).toBe(
      "export",
    );
    expect(resolveShortcut(press({ key: "s", ctrlKey: true }), "mac", false)).toBeNull();
    expect(resolveShortcut(press({ key: "s", metaKey: true }), "mac", false)?.command).toBe(
      "export",
    );
  });

  it("matches letters case-insensitively and through the physical key", () => {
    expect(
      resolveShortcut(press({ key: "B", ctrlKey: true, shiftKey: true }), "windows", false)
        ?.command,
    ).toBe("validate");
    // A layout that produces another character for S still reaches Export.
    expect(
      resolveShortcut(press({ key: "ы", code: "KeyS", ctrlKey: true }), "windows", false)?.command,
    ).toBe("export");
  });

  it("keeps function keys live while typing but leaves editing chords alone", () => {
    expect(resolveShortcut(press({ key: "F5" }), "windows", true)?.command).toBe("run");
    expect(
      resolveShortcut(press({ key: "b", ctrlKey: true, shiftKey: true }), "windows", true)?.command,
    ).toBe("validate");
    // Ctrl+Z in a focused field stays the field's own undo.
    expect(resolveShortcut(press({ key: "z", ctrlKey: true }), "windows", true)).toBeNull();
    expect(
      resolveShortcut(press({ key: "z", ctrlKey: true, shiftKey: true }), "windows", true),
    ).toBeNull();
  });

  it("recognises text entry targets", () => {
    expect(isTypingTarget(document.createElement("textarea"))).toBe(true);
    expect(isTypingTarget(document.createElement("input"))).toBe(true);
    expect(isTypingTarget(document.createElement("select"))).toBe(true);
    expect(isTypingTarget(document.createElement("button"))).toBe(false);
    expect(isTypingTarget(window)).toBe(false);
  });

  it("prints keys the way each platform writes them", () => {
    const [run, cancel, restart] = [
      bindingsFor("run")[0],
      bindingsFor("cancel")[0],
      bindingsFor("restart")[0],
    ];
    expect(shortcutLabel(run, "windows")).toBe("F5");
    expect(shortcutLabel(cancel, "windows")).toBe("Shift+F5");
    expect(shortcutLabel(restart, "windows")).toBe("Ctrl+Shift+F5");
    expect(shortcutLabel(restart, "mac")).toBe("⌘⇧F5");
    expect(shortcutLabelFor("export", "windows")).toBe("Ctrl+S");
    expect(shortcutLabelFor("pause", "windows")).toBeNull();
  });
});

describe("keyboard layer", () => {
  const disposers: Array<() => void> = [];

  afterEach(() => {
    for (const dispose of disposers.splice(0)) dispose();
    for (const dialog of document.querySelectorAll("dialog[open]")) {
      dialog.removeAttribute("open");
    }
    document.body.replaceChildren();
  });

  it("claims F5 for Run so the browser cannot reload the page", () => {
    const invoke = vi.fn();
    disposers.push(installShortcuts({ isEnabled: () => true, invoke }, "windows"));

    const claimed = window.dispatchEvent(press({ key: "F5" }));

    expect(claimed).toBe(false); // preventDefault ran: no reload, no beep
    expect(invoke).toHaveBeenCalledWith("run");
  });

  it("claims a disabled command's key without running it", () => {
    const invoke = vi.fn();
    disposers.push(installShortcuts({ isEnabled: () => false, invoke }, "windows"));

    expect(window.dispatchEvent(press({ key: "F5" }))).toBe(false);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("leaves unbound keys to the rest of the application", () => {
    const invoke = vi.fn();
    disposers.push(installShortcuts({ isEnabled: () => true, invoke }, "windows"));

    // F9 belongs to the canvas, Ctrl+P to the browser.
    expect(window.dispatchEvent(press({ key: "F9" }))).toBe(true);
    expect(window.dispatchEvent(press({ key: "p", ctrlKey: true }))).toBe(true);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("leaves editing chords to the focused field", () => {
    const invoke = vi.fn();
    disposers.push(installShortcuts({ isEnabled: () => true, invoke }, "windows"));
    const field = document.createElement("textarea");
    document.body.appendChild(field);

    expect(field.dispatchEvent(press({ key: "z", ctrlKey: true }))).toBe(true);
    // A function key still belongs to the Studio while typing.
    expect(field.dispatchEvent(press({ key: "F5" }))).toBe(false);
    expect(invoke.mock.calls.map(([command]) => command)).toEqual(["run"]);
  });

  it("does not run a command while a modal dialog is open", () => {
    const dialog = document.createElement("dialog");
    document.body.appendChild(dialog);
    dialog.setAttribute("open", "");
    const invoke = vi.fn();
    disposers.push(installShortcuts({ isEnabled: () => true, invoke }, "windows"));

    expect(window.dispatchEvent(press({ key: "F5" }))).toBe(false);
    expect(invoke).not.toHaveBeenCalled();

    dialog.removeAttribute("open");
    window.dispatchEvent(press({ key: "F5" }));
    expect(invoke).toHaveBeenCalledWith("run");
  });

  it("routes every run key to its own command", () => {
    const invoke = vi.fn();
    disposers.push(installShortcuts({ isEnabled: () => true, invoke }, "windows"));

    window.dispatchEvent(press({ key: "F5", ctrlKey: true, shiftKey: true }));
    window.dispatchEvent(press({ key: "F5", shiftKey: true }));
    window.dispatchEvent(press({ key: "F10" }));
    window.dispatchEvent(press({ key: "b", ctrlKey: true, shiftKey: true }));

    expect(invoke.mock.calls.map(([command]) => command)).toEqual([
      "restart",
      "cancel",
      "step",
      "validate",
    ]);
  });

  it("stops listening once disposed", () => {
    const invoke = vi.fn();
    const dispose = installShortcuts({ isEnabled: () => true, invoke }, "windows");
    dispose();

    expect(window.dispatchEvent(press({ key: "F5" }))).toBe(true);
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe("toolbar wiring", () => {
  const page = new DOMParser().parseFromString(pageSource, "text/html");

  it("has a button for every toolbar command", () => {
    // main.ts looks these up by id and throws when one is missing.
    for (const { command, buttonId } of TOOLBAR_COMMANDS) {
      expect(page.querySelector(`#${buttonId}`), `${command} → #${buttonId}`).not.toBeNull();
    }
  });

  it("only binds keys to commands the toolbar can run", () => {
    for (const binding of SHORTCUTS) {
      expect(
        TOOLBAR_COMMANDS.some((entry) => entry.command === binding.command),
        binding.command,
      ).toBe(true);
    }
  });
});

describe("toolbar hints", () => {
  const BUTTONS = TOOLBAR_COMMANDS.map(
    ({ buttonId }) => `<button id="${buttonId}" type="button"></button>`,
  ).join("");

  afterEach(() => document.body.replaceChildren());

  function toolbar(): HTMLElement {
    document.body.innerHTML = `<header>${BUTTONS}</header>`;
    // The real app has already translated the buttons by this point.
    document.getElementById("btn-run")!.textContent = "Run";
    document.getElementById("btn-cancel")!.textContent = "Cancel";
    document.getElementById("btn-restart")!.textContent = "Restart";
    document.getElementById("btn-step")!.textContent = "Step";
    document.getElementById("btn-step")!.setAttribute("title", "Run one node.");
    document.getElementById("btn-export")!.textContent = "Export";
    return document.querySelector("header")!;
  }

  it("advertises the key on the button that runs the command", () => {
    annotateToolbarShortcuts(toolbar(), "windows");

    expect(document.getElementById("btn-run")!.dataset.shortcut).toBe("F5");
    expect(document.getElementById("btn-run")!.getAttribute("title")).toBe("Run (F5)");
    expect(document.getElementById("btn-cancel")!.dataset.shortcut).toBe("Shift+F5");
    expect(document.getElementById("btn-restart")!.getAttribute("title")).toBe(
      "Restart (Ctrl+Shift+F5)",
    );
    // An existing description is kept and the key is appended to it.
    expect(document.getElementById("btn-step")!.getAttribute("title")).toBe("Run one node. (F10)");
    expect(document.getElementById("btn-pause")!.dataset.shortcut).toBeUndefined();
  });

  it("prints only the keys pressed without looking, so the toolbar stays narrow", () => {
    annotateToolbarShortcuts(toolbar(), "windows");

    expect(
      [...document.querySelectorAll("[data-shortcut]")].map((button) => button.id).sort(),
    ).toEqual(["btn-cancel", "btn-run"]);
    // Everything else keeps its key in the tooltip.
    expect(document.getElementById("btn-export")!.getAttribute("title")).toBe("Export (Ctrl+S)");
    expect(document.getElementById("btn-export")!.getAttribute("aria-keyshortcuts")).toBe(
      "Control+S",
    );
    expect(document.getElementById("btn-step")!.getAttribute("aria-keyshortcuts")).toBe("F10");
  });

  it("exposes every binding to assistive technology", () => {
    annotateToolbarShortcuts(toolbar(), "windows");

    expect(document.getElementById("btn-redo")!.getAttribute("aria-keyshortcuts")).toBe(
      "Control+Y Control+Shift+Z",
    );
    expect(document.getElementById("btn-run")!.getAttribute("aria-keyshortcuts")).toBe("F5");
  });

  it("survives retranslation and repeated annotation", () => {
    const root = toolbar();
    annotateToolbarShortcuts(root, "windows");
    annotateToolbarShortcuts(root, "windows");
    // Translating a button replaces its text; the hint is an attribute, so it
    // is drawn by CSS and cannot be erased that way.
    document.getElementById("btn-run")!.textContent = "运行";

    expect(document.getElementById("btn-run")!.getAttribute("title")).toBe("Run (F5)");
    expect(document.getElementById("btn-run")!.dataset.shortcut).toBe("F5");
  });
});
