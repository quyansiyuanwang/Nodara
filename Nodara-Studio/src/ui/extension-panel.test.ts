import { beforeEach, describe, expect, it } from "vitest";

import { ExtensionPanel } from "./extension-panel";
import { ExtensionDescriptor } from "../runtime/types";

function extension(overrides: Partial<ExtensionDescriptor> = {}): ExtensionDescriptor {
  return {
    id: "nodara.builtins",
    name: "Nodara built-ins",
    version: "2.0.0",
    kind: "builtin",
    source: "runtime",
    capabilities: ["Core"],
    permissions: [],
    node_types: ["core.Start", "core.Log"],
    loaded: true,
    ...overrides,
  };
}

describe("extension panel", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  it("lists unified extension registrations", () => {
    const root = document.createElement("div");
    document.body.appendChild(root);
    new ExtensionPanel(root).setExtensions([
      extension(),
      extension({
        id: "nodara.example",
        name: "Example plugin",
        kind: "plugin",
        source: "plugin",
        node_types: ["example.Node"],
        loaded: false,
      }),
    ]);

    const rows = root.querySelectorAll("tbody tr");
    expect(rows).toHaveLength(2);
    expect(rows[0].textContent).toContain("Nodara built-ins");
    expect(rows[0].textContent).toContain("built-in");
    expect(rows[0].textContent).toContain("loaded");
    expect(rows[1].textContent).toContain("plugin");
    expect(rows[1].textContent).toContain("discovered");
  });

  it("shows an empty state", () => {
    const root = document.createElement("div");
    document.body.appendChild(root);
    new ExtensionPanel(root).setExtensions([]);
    expect(root.textContent).toContain("No extensions");
  });
});
