import { describe, expect, it } from "vitest";

import { deriveRunControls } from "./run-controls";

describe("run control state", () => {
  it("stays runnable when the editor is idle and validation passes", () => {
    const controls = deriveRunControls(null, true, "valid", 0);
    expect(controls.runDisabled).toBe(false);
    expect(controls.stepDisabled).toBe(false);
    expect(controls.cancelDisabled).toBe(true);
  });

  it("disables Run while a run is in flight", () => {
    expect(deriveRunControls("pending", true, "valid", 0).runDisabled).toBe(true);
    expect(deriveRunControls("running", true, "valid", 0).runDisabled).toBe(true);
    expect(deriveRunControls("paused", true, "valid", 0).runDisabled).toBe(true);
  });

  it("enables Step while paused or idle and disables it while running", () => {
    expect(deriveRunControls("paused", true, "valid", 0).stepDisabled).toBe(false);
    expect(deriveRunControls(null, true, "valid", 0).stepDisabled).toBe(false);
    expect(deriveRunControls("completed", true, "valid", 0).stepDisabled).toBe(false);
    expect(deriveRunControls("pending", true, "valid", 0).stepDisabled).toBe(true);
    expect(deriveRunControls("running", true, "valid", 0).stepDisabled).toBe(true);
  });

  it("disables Run when validation or local edits block execution", () => {
    expect(deriveRunControls(null, true, "invalid", 0).runDisabled).toBe(true);
    expect(deriveRunControls(null, true, "valid", 1).runDisabled).toBe(true);
    expect(deriveRunControls(null, false, "valid", 0).runDisabled).toBe(true);
  });

  it("keeps Run clickable while an automatic validation pass is pending", () => {
    expect(deriveRunControls(null, true, "checking", 0).runDisabled).toBe(false);
  });

  it("only offers Restart while a run is in flight", () => {
    expect(deriveRunControls(null, true, "valid", 0).restartDisabled).toBe(true);
    expect(deriveRunControls("completed", true, "valid", 0).restartDisabled).toBe(true);
    expect(deriveRunControls("failed", true, "valid", 0).restartDisabled).toBe(true);
    expect(deriveRunControls("pending", true, "valid", 0).restartDisabled).toBe(false);
    expect(deriveRunControls("running", true, "valid", 0).restartDisabled).toBe(false);
    expect(deriveRunControls("paused", true, "valid", 0).restartDisabled).toBe(false);
  });

  it("names why Run cannot start", () => {
    // A silent grey button is the whole feedback otherwise, and an unreachable
    // runtime or a missing plugin node type is easy to miss.
    expect(deriveRunControls(null, false, "valid", 0).runBlockReason).toBe("offline");
    expect(deriveRunControls(null, true, "valid", 2).runBlockReason).toBe("localProblems");
    expect(deriveRunControls(null, true, "invalid", 0).runBlockReason).toBe("validation");
    expect(deriveRunControls(null, true, "unavailable", 0).runBlockReason).toBe("validation");
    expect(deriveRunControls(null, true, "valid", 0).runBlockReason).toBeNull();
    expect(deriveRunControls(null, true, "checking", 0).runBlockReason).toBeNull();
    // An in-flight run needs no explanation.
    expect(deriveRunControls("running", true, "valid", 0).runBlockReason).toBeNull();
    expect(deriveRunControls("paused", false, "valid", 0).runBlockReason).toBeNull();
  });
});
