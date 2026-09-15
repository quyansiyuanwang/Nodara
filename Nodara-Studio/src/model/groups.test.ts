import { describe, expect, it } from "vitest";

import {
  assignNodesToGroup,
  createWorkflowGroup,
  deleteWorkflowGroup,
  getWorkflowGroups,
  groupBoundingBox,
  groupForNode,
  pruneWorkflowGroups,
  renameNodeInGroups,
  STUDIO_GROUPS_EXTENSION,
} from "./groups";
import { emptyWorkflow } from "./workflow";

function workflow() {
  const value = emptyWorkflow();
  value.nodes = [
    { id: "start", type: "core.Start", config: {}, position: { x: 10, y: 20 } },
    { id: "log", type: "core.Log", config: {}, position: { x: 260, y: 20 } },
  ];
  return value;
}

describe("workflow groups", () => {
  it("persists groups in the workflow extension bag", () => {
    const value = workflow();
    createWorkflowGroup(value, ["start", "log"], "Capture flow", "#3fb27f");

    expect(value.metadata.extensions?.[STUDIO_GROUPS_EXTENSION]).toHaveLength(1);
    expect(getWorkflowGroups(value)).toEqual([
      { id: "group-1", name: "Capture flow", color: "#3fb27f", node_ids: ["start", "log"] },
    ]);
    expect(groupForNode(value, "log")?.id).toBe("group-1");
  });

  it("moves nodes between groups instead of duplicating membership", () => {
    const value = workflow();
    createWorkflowGroup(value, ["start"], "One");
    createWorkflowGroup(value, ["log"], "Two");

    assignNodesToGroup(value, "group-2", ["start"]);

    expect(getWorkflowGroups(value).map((group) => group.node_ids)).toEqual([["log", "start"]]);
  });

  it("renames node references and prunes deleted nodes", () => {
    const value = workflow();
    createWorkflowGroup(value, ["start", "log"], "Flow");
    value.nodes = value.nodes.map((node) => node.id === "start" ? { ...node, id: "begin" } : node);
    renameNodeInGroups(value, "start", "begin");
    value.nodes = value.nodes.filter((node) => node.id !== "log");
    pruneWorkflowGroups(value);

    expect(getWorkflowGroups(value)[0].node_ids).toEqual(["begin"]);
  });

  it("computes a padded frame and removes empty groups", () => {
    const value = workflow();
    const group = createWorkflowGroup(value, ["start", "log"], "Flow");
    expect(groupBoundingBox(value, group, 200, 108)).toEqual({
      x: -10,
      y: -14,
      width: 490,
      height: 160,
    });

    deleteWorkflowGroup(value, group.id);
    expect(getWorkflowGroups(value)).toEqual([]);
    expect(value.metadata.extensions).toBeUndefined();
  });
});
