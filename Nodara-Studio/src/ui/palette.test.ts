import { beforeEach, describe, expect, it } from "vitest";

import { Palette } from "./palette";
import { NodeDescriptor } from "../runtime/types";

function descriptor(
  nodeType: string,
  display: string,
  category: string,
  dangerous = false,
): NodeDescriptor {
  return {
    node_type: nodeType,
    display_name: display,
    category,
    description: `${display} description`,
    version: "2.0.0",
    inputs: [],
    outputs: [],
    config_schema: {},
    capabilities: [],
    permissions: dangerous ? ["input.control"] : [],
    dangerous,
    allows_additional_config: true,
  };
}

/**
 * The palette must be built entirely from what the runtime reports: installing a
 * plugin and restarting the runtime is the only step needed to change it.
 */
describe("dynamic node discovery in the palette", () => {
  let host: HTMLElement;

  beforeEach(() => {
    document.body.innerHTML = "";
    host = document.createElement("div");
    document.body.appendChild(host);
  });

  it("renders whatever the runtime reports, grouped by category", () => {
    const palette = new Palette(host, { onAdd: () => undefined });
    palette.setDescriptors([
      descriptor("core.Start", "Start", "Core"),
      descriptor("core.Log", "Log", "Core"),
      descriptor("windows.Input.Keyboard", "Keyboard", "Input", true),
    ]);

    const categories = [...host.querySelectorAll(".palette__category")].map(
      (element) => element.textContent,
    );
    expect(categories).toEqual(["Core", "Input"]);
    expect(host.querySelectorAll(".palette__item")).toHaveLength(3);
  });

  it("badges gated nodes with the permission they need", () => {
    const palette = new Palette(host, { onAdd: () => undefined });
    palette.setDescriptors([descriptor("windows.Input.Keyboard", "Keyboard", "Input", true)]);
    const badge = host.querySelector(".badge--warn")!;
    expect(badge.textContent).toBe("gated");
    expect(badge.getAttribute("title")).toContain("input.control");
  });

  it("adds a node when the item is activated", () => {
    const added: string[] = [];
    const palette = new Palette(host, {
      onAdd: (descriptor) => added.push(descriptor.node_type),
    });
    palette.setDescriptors([descriptor("core.Log", "Log", "Core")]);

    const item = host.querySelector<HTMLButtonElement>(".palette__item")!;
    item.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(added).toEqual(["core.Log"]);
  });

  it("starts a pointer drag when the item is pulled toward the canvas", () => {
    const dragged: string[] = [];
    const palette = new Palette(host, {
      onAdd: () => undefined,
      onDragStart: (descriptor) => dragged.push(descriptor.node_type),
    });
    palette.setDescriptors([descriptor("core.Log", "Log", "Core")]);

    const item = host.querySelector<HTMLButtonElement>(".palette__item")!;
    item.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, button: 0 }));
    expect(dragged).toEqual(["core.Log"]);
  });

  it("filters across name, type and description", () => {
    const palette = new Palette(host, { onAdd: () => undefined });
    palette.setDescriptors([
      descriptor("core.Log", "Log", "Core"),
      descriptor("vision.Ocr", "OCR", "Vision"),
    ]);

    palette.filter("ocr");
    const items = host.querySelectorAll(".palette__item");
    expect(items).toHaveLength(1);
    expect(items[0].textContent).toContain("OCR");

    palette.filter("nothing matches this");
    expect(host.textContent).toContain("No node types match");
  });

  it("tells the operator when the runtime reported nothing", () => {
    const palette = new Palette(host, { onAdd: () => undefined });
    palette.setDescriptors([]);
    expect(host.textContent).toContain("Is the runtime running?");
  });
});
