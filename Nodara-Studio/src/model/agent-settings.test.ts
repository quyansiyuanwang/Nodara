import { beforeEach, describe, expect, it } from "vitest";
import { setLocale } from "../i18n";
import {
  PROVIDER_PRESETS,
  allTemplates,
  builtinTemplates,
  defaultAgentSettings,
  findTemplate,
  isBuiltinTemplate,
  loadAgentSettings,
  saveAgentSettings,
} from "./agent-settings";

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

  it("ships usable prompt templates in both languages", () => {
    for (const locale of ["en", "zh-CN"] as const) {
      setLocale(locale);
      const templates = builtinTemplates();
      expect(templates.length).toBeGreaterThanOrEqual(4);
      for (const template of templates) {
        expect(isBuiltinTemplate(template.id)).toBe(true);
        // A template that renders as a raw key or as an empty constraint is
        // worse than none at all.
        expect(template.name).not.toContain("agent.");
        expect(template.instructions.length).toBeGreaterThan(40);
      }
      expect(new Set(templates.map((template) => template.id)).size).toBe(templates.length);
    }
    setLocale("en");
  });

  it("keeps built-in templates out of persisted settings", () => {
    const settings = defaultAgentSettings();
    expect(settings.templates).toEqual([]);
    expect(allTemplates(settings.templates)).toEqual(builtinTemplates());
  });

  it("resolves an operator template that shares no id with a built-in one", () => {
    const settings = defaultAgentSettings();
    settings.templates = [{ id: "mine", name: "Mine", instructions: "be terse" }];
    expect(findTemplate("mine", settings.templates)?.instructions).toBe("be terse");
    expect(findTemplate("missing", settings.templates)).toBeUndefined();
    expect(findTemplate(undefined, settings.templates)).toBeUndefined();
  });

  it("offers OpenAI-compatible presets without creating profiles", () => {
    const settings = defaultAgentSettings();
    const before = settings.profiles.length;
    expect(PROVIDER_PRESETS.length).toBeGreaterThanOrEqual(3);
    for (const preset of PROVIDER_PRESETS) {
      expect(preset.endpoint).toMatch(/^https?:\/\//);
      expect(preset.model.length).toBeGreaterThan(0);
    }
    expect(settings.profiles.length).toBe(before);
  });
});
