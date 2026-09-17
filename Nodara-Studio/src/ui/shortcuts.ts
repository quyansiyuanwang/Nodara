/**
 * Keyboard shortcuts for the Studio toolbar, modelled on Visual Studio.
 *
 * This module owns the *binding table* only; the command bodies live in
 * `main.ts`. Because both a toolbar button and its key dispatch the same
 * command id, a button and its shortcut can never drift apart, and the toolbar
 * can advertise the key that fires it.
 *
 * Two rules make the table safe in a browser-hosted Studio and in WebView2:
 * every matched key is claimed with `preventDefault`, and a matched key is
 * always claimed even when its command is unavailable. F5 therefore never
 * reloads the page — a run that cannot start must not throw the document away.
 */

export type Platform = "windows" | "mac" | "other";

export interface ShortcutBinding {
  /** Command id, resolved through `TOOLBAR_COMMANDS`. */
  command: string;
  /** `KeyboardEvent.key`; letters are matched case-insensitively. */
  key: string;
  /** Ctrl on Windows and Linux, Cmd on macOS. */
  ctrl?: boolean;
  shift?: boolean;
  /**
   * Fire even while a text field has focus. Visual Studio keeps its function
   * keys live while editing, and a browser would otherwise claim Ctrl+S or
   * Ctrl+N for its own UI. Editing chords such as Ctrl+Z stay out of the way so
   * the focused field keeps its own undo history.
   */
  whileTyping?: boolean;
  /**
   * Print this key on its toolbar button.
   *
   * Only Start and Stop are printed: those are the keys an operator presses
   * without looking at the toolbar, and the compact toolbar is already close to
   * the width of a 1440-wide window, so printing the rest would push Pause,
   * Step and Cancel out of view. Every other binding still reaches the button's
   * tooltip and `aria-keyshortcuts`.
   */
  printed?: boolean;
}

/**
 * Visual Studio's debug keys, applied to the run controls.
 *
 * `Ctrl+Shift+B` — VS's Build Solution — validates instead, because validation
 * is what makes a workflow runnable here, and `Ctrl+F5` (Start Without
 * Debugging) opens the per-run variables before starting.
 */
export const SHORTCUTS: readonly ShortcutBinding[] = [
  { command: "run", key: "F5", whileTyping: true, printed: true },
  { command: "cancel", key: "F5", shift: true, whileTyping: true, printed: true },
  { command: "restart", key: "F5", ctrl: true, shift: true, whileTyping: true },
  { command: "step", key: "F10", whileTyping: true },
  { command: "runOptions", key: "F5", ctrl: true, whileTyping: true },
  { command: "validate", key: "b", ctrl: true, shift: true, whileTyping: true },
  { command: "new", key: "n", ctrl: true, whileTyping: true },
  { command: "import", key: "o", ctrl: true, whileTyping: true },
  { command: "export", key: "s", ctrl: true, whileTyping: true },
  { command: "undo", key: "z", ctrl: true },
  { command: "redo", key: "y", ctrl: true },
  { command: "redo", key: "z", ctrl: true, shift: true },
];

/** Toolbar commands in button order; `buttonId` values match `index.html`. */
export const TOOLBAR_COMMANDS: readonly { command: string; buttonId: string }[] = [
  { command: "new", buttonId: "btn-new" },
  { command: "import", buttonId: "btn-import" },
  { command: "export", buttonId: "btn-export" },
  { command: "undo", buttonId: "btn-undo" },
  { command: "redo", buttonId: "btn-redo" },
  { command: "validate", buttonId: "btn-validate" },
  { command: "run", buttonId: "btn-run" },
  { command: "restart", buttonId: "btn-restart" },
  { command: "runOptions", buttonId: "btn-run-options" },
  { command: "pause", buttonId: "btn-pause" },
  { command: "resume", buttonId: "btn-resume" },
  { command: "step", buttonId: "btn-step" },
  { command: "cancel", buttonId: "btn-cancel" },
];

export interface ShortcutHost {
  /** True when the command would be clickable on the toolbar. */
  isEnabled(command: string): boolean;
  /** Run the command, exactly as clicking its toolbar button would. */
  invoke(command: string): void;
}

export function detectPlatform(): Platform {
  const source = `${globalThis.navigator?.userAgent ?? ""} ${
    (globalThis.navigator as { platform?: string } | undefined)?.platform ?? ""
  }`;
  if (/mac|iphone|ipad/i.test(source)) return "mac";
  if (/windows|win32|win64/i.test(source)) return "windows";
  return "other";
}

export function buttonIdFor(command: string): string | null {
  return TOOLBAR_COMMANDS.find((entry) => entry.command === command)?.buttonId ?? null;
}

/** Bindings for one command, most prominent first. */
export function bindingsFor(command: string): ShortcutBinding[] {
  return SHORTCUTS.filter((binding) => binding.command === command);
}

/**
 * The key a `KeyboardEvent` maps to.
 *
 * Letter chords also accept the physical key, so `Ctrl+S` still saves on a
 * layout where `S` produces a different character.
 */
function matchesKey(binding: ShortcutBinding, event: KeyboardEvent): boolean {
  if (event.key.toLowerCase() === binding.key.toLowerCase()) return true;
  return /^[a-z]$/.test(binding.key) && event.code === `Key${binding.key.toUpperCase()}`;
}

function matchesModifiers(binding: ShortcutBinding, event: KeyboardEvent, platform: Platform): boolean {
  if (event.altKey) return false;
  const ctrl = platform === "mac" ? event.metaKey : event.ctrlKey;
  return (binding.ctrl ?? false) === ctrl && (binding.shift ?? false) === event.shiftKey;
}

/** True when the key event is going into a field the user is editing. */
export function isTypingTarget(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  if (!element || typeof element.tagName !== "string") return false;
  if (["INPUT", "TEXTAREA", "SELECT"].includes(element.tagName)) return true;
  return element.isContentEditable === true;
}

/**
 * Resolve a key event to the binding that owns it.
 *
 * Returns `null` when nothing in the table matches, or when the only match
 * would steal a key from a focused text field.
 */
export function resolveShortcut(
  event: KeyboardEvent,
  platform: Platform,
  typing: boolean,
): ShortcutBinding | null {
  for (const binding of SHORTCUTS) {
    if (!matchesModifiers(binding, event, platform)) continue;
    if (!matchesKey(binding, event)) continue;
    if (typing && !binding.whileTyping) continue;
    return binding;
  }
  return null;
}

export function shortcutLabel(binding: ShortcutBinding, platform: Platform): string {
  const key = binding.key.length === 1 ? binding.key.toUpperCase() : binding.key;
  if (platform === "mac") {
    return `${binding.ctrl ? "⌘" : ""}${binding.shift ? "⇧" : ""}${key}`;
  }
  return `${binding.ctrl ? "Ctrl+" : ""}${binding.shift ? "Shift+" : ""}${key}`;
}

/** Label for the command's most prominent binding, or `null` when it has none. */
export function shortcutLabelFor(command: string, platform: Platform): string | null {
  const [first] = bindingsFor(command);
  return first ? shortcutLabel(first, platform) : null;
}

function ariaShortcut(binding: ShortcutBinding, platform: Platform): string {
  const key = binding.key.length === 1 ? binding.key.toUpperCase() : binding.key;
  const ctrl = platform === "mac" ? "Meta+" : "Control+";
  return `${binding.ctrl ? ctrl : ""}${binding.shift ? "Shift+" : ""}${key}`;
}

/**
 * Publish each toolbar command's key on the button itself: a printed hint drawn
 * by CSS for the bindings that ask for one, and a tooltip plus
 * `aria-keyshortcuts` for every binding.
 *
 * The printed hint is written as a data attribute rather than as button text,
 * so re-applying translations — which replaces that text — cannot drop it.
 */
export function annotateToolbarShortcuts(
  root: ParentNode = document,
  platform: Platform = detectPlatform(),
): void {
  for (const { command, buttonId } of TOOLBAR_COMMANDS) {
    const bindings = bindingsFor(command);
    if (bindings.length === 0) continue;
    const button = root.querySelector<HTMLElement>(`#${buttonId}`);
    if (!button) continue;
    const label = shortcutLabel(bindings[0], platform);
    button.setAttribute(
      "aria-keyshortcuts",
      bindings.map((binding) => ariaShortcut(binding, platform)).join(" "),
    );
    const described = button.getAttribute("title")?.trim() || button.textContent?.trim() || "";
    if (!described.includes(`(${label})`)) {
      button.setAttribute("title", described ? `${described} (${label})` : label);
    }
    if (bindings[0].printed) button.dataset.shortcut = label;
  }
}

/**
 * Install the keyboard layer and return the cleanup callback.
 *
 * The listener runs in the capture phase so the Studio sees the key before any
 * panel, and before the browser can act on it.
 */
export function installShortcuts(
  host: ShortcutHost,
  platform: Platform = detectPlatform(),
): () => void {
  const onKeyDown = (event: KeyboardEvent): void => {
    const binding = resolveShortcut(event, platform, isTypingTarget(event.target));
    if (!binding) return;
    // F5, Ctrl+S and friends must never reach the browser or WebView2 as
    // refresh/save accelerators, whatever the state of the command.
    event.preventDefault();
    event.stopPropagation();
    // A modal dialog owns the keyboard for as long as it is open.
    if (document.querySelector("dialog[open]")) return;
    if (host.isEnabled(binding.command)) host.invoke(binding.command);
  };
  window.addEventListener("keydown", onKeyDown, { capture: true });
  return () => window.removeEventListener("keydown", onKeyDown, { capture: true });
}
