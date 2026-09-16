import { beforeEach, describe, expect, it } from "vitest";
import { emptyWorkflow } from "./workflow";
import {
  applyTheme,
  edgeColor,
  nodeColor,
  setEdgeColor,
  setNodeColor,
  storedTheme,
} from "./visuals";

describe("workflow visual overrides", () => {
  beforeEach(() => localStorage.clear());

  it("stores node and edge colors in metadata extensions", () => {
    const workflow = emptyWorkflow();
    workflow.edges.push({ id: "e1", kind: "control", source: "start", target: "end" });
    setNodeColor(workflow, "start", "#123456");
    setEdgeColor(workflow, "e1", "#abcdef");

    expect(nodeColor(workflow, "start")).toBe("#123456");
    expect(edgeColor(workflow, "e1")).toBe("#abcdef");
    expect(workflow.metadata.extensions?.["studio.visuals"]).toMatchObject({ version: 1 });
  });

  it("restores theme selection", () => {
    applyTheme("paper");
    expect(document.documentElement.dataset.theme).toBe("paper");
    expect(storedTheme()).toBe("paper");
  });
});
