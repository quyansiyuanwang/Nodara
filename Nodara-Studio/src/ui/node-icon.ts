import { Braces, Calculator, Camera, Clipboard, Clock3, createElement, FolderOpen, GitBranch, Keyboard, Mail, Monitor, Play, Puzzle, ScanLine, ScrollText, Square, Terminal } from "lucide";

const kinds = {
  start: [Play, "#44b991"], end: [Square, "#44b991"], log: [ScrollText, "#68a9e5"],
  calculation: [Calculator, "#ae93e9"], variable: [Braces, "#ae93e9"], delay: [Clock3, "#dfad58"],
  flow: [GitBranch, "#dfad58"], image: [Camera, "#57bcc8"], vision: [ScanLine, "#57bcc8"],
  desktop: [Monitor, "#68a9e5"], input: [Keyboard, "#dd956c"], command: [Terminal, "#dd956c"],
  clipboard: [Clipboard, "#dd956c"], inventory: [FolderOpen, "#57bcc8"], mail: [Mail, "#dfad58"],
  plugin: [Puzzle, "#bc91bb"],
} as const;

function family(type: string): keyof typeof kinds {
  if (type === "core.Start") return "start";
  if (type === "core.End") return "end";
  if (type === "core.Log") return "log";
  if (type.includes("Calculate")) return "calculation";
  if (type.includes("Variable")) return "variable";
  if (type.endsWith(".Delay")) return "delay";
  if (type.includes("Capture")) return "image";
  if (type.startsWith("vision.")) return "vision";
  if (type.includes(".Window.")) return "desktop";
  if (type.includes(".Input.")) return "input";
  if (type === "system.Command") return "command";
  if (type === "system.Clipboard") return "clipboard";
  if (type === "desktop.ScreenshotIndex") return "inventory";
  if (type.startsWith("mail.")) return "mail";
  return type.startsWith("core.") ? "flow" : "plugin";
}

export function nodeIcon(type: string, size: number) {
  const [glyph, color] = kinds[family(type)];
  return { color, icon: createElement(glyph, { width: size, height: size, "aria-hidden": "true", "pointer-events": "none" }) };
}
