import { describe, expect, it } from "vitest";

import { FeatureRegistry } from "./feature-registry";

describe("Studio feature registry", () => {
  it("sorts panels by order and supports replacement", () => {
    const registry = new FeatureRegistry();
    registry.registerPanel({ id: "events", labelKey: "tabs.events", panelId: "panel-events", order: 20 });
    registry.registerPanel({ id: "runs", labelKey: "tabs.runs", panelId: "panel-runs", order: 10 });
    expect(registry.panels().map((panel) => panel.id)).toEqual(["runs", "events"]);

    registry.registerPanel({ id: "runs", labelKey: "tabs.extensions", panelId: "panel-extensions", order: 5 });
    expect(registry.getPanel("runs")).toMatchObject({
      labelKey: "tabs.extensions",
      panelId: "panel-extensions",
    });
    expect(registry.size).toBe(2);
  });
});
