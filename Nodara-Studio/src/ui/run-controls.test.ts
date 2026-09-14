import { describe, expect, it } from "vitest";

import { deriveRunControls } from "./run-controls";

describe("run control state", () => {
  it("stays runnable when the editor is idle and validation passes", () => {
    const controls = deriveRunControls(null, true, "valid", 0);
    expect(controls.runDisabled).toBe(false);
    expect(controls.cancelDisabled).toBe(true);
  });

  it("disables Run while a run is in flight", () => {
    expect(deriveRunControls("pending", true, "valid", 0).runDisabled).toBe(true);
    expect(deriveRunControls("running", true, "valid", 0).runDisabled).toBe(true);
    expect(deriveRunControls("paused", true, "valid", 0).runDisabled).toBe(true);
  });

  it("disables Run when validation or local edits block execution", () => {
    expect(deriveRunControls(null, true, "invalid", 0).runDisabled).toBe(true);
    expect(deriveRunControls(null, true, "valid", 1).runDisabled).toBe(true);
    expect(deriveRunControls(null, false, "valid", 0).runDisabled).toBe(true);
  });

  it("keeps Run clickable while an automatic validation pass is pending", () => {
    expect(deriveRunControls(null, true, "checking", 0).runDisabled).toBe(false);
  });
});
