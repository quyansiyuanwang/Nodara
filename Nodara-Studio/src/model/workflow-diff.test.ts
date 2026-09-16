import { describe, expect, it } from "vitest";
import { starterWorkflow } from "./workflow";
import { diffWorkflows, isWorkflowDiffEmpty } from "./workflow-diff";

describe("workflow plan diff", () => {
  it("reports added, changed and removed graph elements", () => {
    const base = starterWorkflow();
    const next = structuredClone(base);
    next.nodes[1].label = "Changed";
    next.nodes.push({ id: "extra", type: "core.Log", label: "Extra", config: {} });
    next.edges.push({ id: "extra-edge", kind: "control", source: "hello", target: "extra" });
    const diff = diffWorkflows(base, next);
    expect(diff.nodesChanged.map((entry) => entry.id)).toContain("hello");
    expect(diff.nodesAdded.map((entry) => entry.id)).toContain("extra");
    expect(diff.edgesAdded.map((entry) => entry.id)).toContain("extra-edge");
    expect(isWorkflowDiffEmpty(diff)).toBe(false);
  });
});
