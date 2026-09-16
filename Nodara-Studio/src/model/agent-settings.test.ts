import { beforeEach, describe, expect, it } from "vitest";
import { defaultAgentSettings, loadAgentSettings, saveAgentSettings } from "./agent-settings";

describe("agent settings", () => {
  beforeEach(() => localStorage.clear());

  it("persists profiles and workspace settings without secrets", () => {
    const settings = defaultAgentSettings();
    settings.profiles[0].model = "local-model";
    settings.mode = "manual";
    settings.workspaceOpen = true;
    saveAgentSettings(settings);
    const loaded = loadAgentSettings();
    expect(loaded.profiles[0].model).toBe("local-model");
    expect(loaded.mode).toBe("manual");
    expect(loaded.workspaceOpen).toBe(true);
    expect(JSON.stringify(loaded)).not.toContain("apiKey");
  });
});
