import { beforeEach, describe, expect, it, vi } from "vitest";

import { RegionPicker } from "./region-picker";

function pointer(type: string, x: number, y: number): PointerEvent {
  return new MouseEvent(type, {
    bubbles: true,
    button: 0,
    clientX: x,
    clientY: y,
  }) as unknown as PointerEvent;
}

describe("screen region picker", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  it("captures a desktop image and converts a drag into pixel coordinates", async () => {
    const root = document.createElement("dialog");
    Object.defineProperty(root, "showModal", { value: () => undefined });
    Object.defineProperty(root, "close", { value: () => undefined });
    document.body.appendChild(root);
    const client = {
      createRun: vi.fn().mockResolvedValue({ id: "r1", status: "running" }),
      getRun: vi.fn().mockResolvedValue({ id: "r1", status: "completed" }),
      listArtifacts: vi.fn().mockResolvedValue([
        { id: "image-1", name: "desktop", content_type: "image/png", size: 10 },
      ]),
      artifactUrl: vi.fn().mockReturnValue("/artifact.png"),
    };
    const picker = new RegionPicker(root, client);
    const result = picker.pick();
    await vi.waitFor(() => expect(root.querySelector("img")).not.toBeNull());

    const image = root.querySelector<HTMLImageElement>("img")!;
    Object.defineProperty(image, "naturalWidth", { configurable: true, value: 1000 });
    Object.defineProperty(image, "naturalHeight", { configurable: true, value: 500 });
    image.getBoundingClientRect = () => ({
      left: 0,
      top: 0,
      right: 500,
      bottom: 250,
      width: 500,
      height: 250,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    });
    image.setPointerCapture = () => undefined;
    image.dispatchEvent(pointer("pointerdown", 100, 50));
    image.dispatchEvent(pointer("pointermove", 300, 150));
    image.dispatchEvent(pointer("pointerup", 300, 150));
    root.querySelector<HTMLButtonElement>(".btn--primary")!.click();

    await expect(result).resolves.toEqual({ x: 200, y: 100, width: 400, height: 200 });
  });
});
