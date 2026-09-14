import { beforeEach, describe, expect, it } from "vitest";

import { installResizer } from "./resizer";

describe("panel resizers", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  it("resizes from the keyboard and respects bounds", () => {
    const handle = document.createElement("div");
    document.body.appendChild(handle);
    let value = 240;
    installResizer(handle, {
      axis: "x",
      value,
      min: 200,
      max: () => 280,
      onChange: (next) => {
        value = next;
      },
    });

    handle.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight" }));
    expect(value).toBe(256);

    for (let index = 0; index < 10; index += 1) {
      handle.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight" }));
    }
    expect(value).toBe(280);
  });

  it("inverts keyboard direction for a right or bottom edge", () => {
    const handle = document.createElement("div");
    document.body.appendChild(handle);
    let value = 300;
    installResizer(handle, {
      axis: "x",
      value,
      min: 100,
      max: () => 500,
      invert: true,
      onChange: (next) => {
        value = next;
      },
    });

    handle.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft" }));
    expect(value).toBe(316);
  });
});
