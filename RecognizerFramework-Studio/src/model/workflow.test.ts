import { describe, expect, it } from "vitest";

import {
  applyWorkflow,
  defaultConfig,
  edgeExists,
  emptyWorkflow,
  localProblems,
  nextEdgeId,
  nextNodeId,
  WORKFLOW_SCHEMA_PATH,
} from "./workflow";
import { NodeDescriptor, Workflow } from "../runtime/types";

function descriptor(nodeType: string): NodeDescriptor {
  return {
    node_type: nodeType,
    display_name: nodeType,
    category: "Test",
    description: "",
    version: "",
    inputs: [],
    outputs: [],
    config_schema: {},
    capabilities: [],
    permissions: [],
    dangerous: false,
    allows_additional_config: true,
  };
}

describe("workflow model", () => {
  it("starts from a usable scaffold", () => {
    const workflow = emptyWorkflow();
    expect(workflow.schema_version).toBe("2.0");
    expect(workflow.$schema).toBe(WORKFLOW_SCHEMA_PATH);
    expect(workflow.nodes.map((node) => node.type)).toEqual(["core.Start", "core.End"]);
    expect(localProblems(workflow)).toEqual([]);
  });

  it("keeps the schema reference of an imported document", () => {
    const workflow = emptyWorkflow();
    const imported: Partial<Workflow> = {
      $schema: "http://127.0.0.1:8710/api/v1/schema/workflow",
      id: "wf.imported",
      nodes: [{ id: "n", type: "core.Log", config: {} }],
    };
    const same = applyWorkflow(workflow, imported);

    // The editor mutates one instance, so the canvas keeps its reference.
    expect(same).toBe(workflow);
    expect(workflow.$schema).toBe("http://127.0.0.1:8710/api/v1/schema/workflow");
    expect(workflow.id).toBe("wf.imported");
    expect(workflow.nodes).toHaveLength(1);
  });

  it("falls back to the published schema when a document omits one", () => {
    const workflow = emptyWorkflow();
    applyWorkflow(workflow, { id: "wf.plain" });
    expect(workflow.$schema).toBe(WORKFLOW_SCHEMA_PATH);
    expect(workflow.schema_version).toBe("2.0");
  });

  it("generates unique ids", () => {
    const workflow = emptyWorkflow();
    expect(nextNodeId(descriptor("windows.Input.Keyboard"), workflow.nodes)).toBe("keyboard");
    workflow.nodes.push({ id: "keyboard", type: "windows.Input.Keyboard", config: {} });
    const second = nextNodeId(descriptor("windows.Input.Keyboard"), workflow.nodes);
    expect(second).not.toBe("keyboard");

    expect(nextEdgeId("a", "b", [])).toBe("a-b");
    const existing = [{ id: "a-b", source: "a", target: "b" }];
    expect(nextEdgeId("a", "b", existing)).not.toBe("a-b");
  });

  it("seeds configuration from the descriptor schema", () => {
    const withDefaults: NodeDescriptor = {
      ...descriptor("core.Log"),
      config_schema: {
        type: "object",
        properties: {
          message: { type: "string" },
          level: { type: "string", default: "info" },
        },
        required: ["message"],
      },
    };
    expect(defaultConfig(withDefaults)).toEqual({ level: "info" });
  });

  it("detects duplicate connections but distinguishes ports", () => {
    const edges = [
      { id: "e1", source: "a", target: "b" },
      { id: "e2", source: "a", target: "c", source_port: "other" },
    ];
    expect(edgeExists(edges, "a", "b")).toBe(true);
    expect(edgeExists(edges, "a", "b", "out", "in")).toBe(true);
    expect(edgeExists(edges, "a", "c")).toBe(false);
    expect(edgeExists(edges, "a", "c", "other", "in")).toBe(true);
  });

  it("reports the mistakes an editor can catch locally", () => {
    const workflow: Workflow = {
      schema_version: "2.0",
      id: "wf.bad",
      metadata: { name: "bad", tags: [] },
      nodes: [
        { id: "start", type: "core.Start", config: {} },
        { id: "start", type: "core.Log", config: {} },
      ],
      edges: [{ id: "e1", source: "start", target: "ghost" }],
      variables: {},
    };
    const problems = localProblems(workflow);
    expect(problems.some((line) => line.includes("duplicate node id"))).toBe(true);
    expect(problems.some((line) => line.includes("missing target"))).toBe(true);
    expect(problems.some((line) => line.includes("no core.End"))).toBe(true);
  });
});
