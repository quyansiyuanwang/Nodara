import { describe, expect, it } from "vitest";

import {
  applyWorkflow,
  defaultConfig,
  edgeExists,
  emptyWorkflow,
  localProblems,
  migrateLegacyWorkflow,
  starterWorkflow,
  nextEdgeId,
  nextNodeId,
  nodeTypeAdmission,
  valueTypesCompatible,
  workflowAdmission,
  WORKFLOW_SCHEMA_PATH,
} from "./workflow";
import { NodeDescriptor, Workflow, WorkflowEdge } from "../runtime/types";

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
    expect(workflow.schema_version).toBe("2.1");
    expect(workflow.$schema).toBe(WORKFLOW_SCHEMA_PATH);
    expect(workflow.nodes.map((node) => node.type)).toEqual(["core.Start", "core.End"]);
    expect(localProblems(workflow)).toEqual([]);
  });

  it("provides a runnable starter workflow", () => {
    const workflow = starterWorkflow();
    expect(workflow.nodes.map((node) => node.type)).toEqual([
      "core.Start",
      "core.Log",
      "core.End",
    ]);
    expect(workflow.edges).toHaveLength(2);
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
    expect(workflow.schema_version).toBe("2.1");
  });

  it("admits only one core.Start node", () => {
    const workflow = emptyWorkflow();
    expect(nodeTypeAdmission(workflow, "core.Start").allowed).toBe(false);
    expect(nodeTypeAdmission(workflow, "core.Log").allowed).toBe(true);

    workflow.nodes = workflow.nodes.filter((node) => node.type !== "core.Start");
    expect(nodeTypeAdmission(workflow, "core.Start").allowed).toBe(true);
  });

  it("rejects a document that declares multiple core.Start nodes", () => {
    const workflow = emptyWorkflow();
    workflow.nodes.push({ id: "start2", type: "core.Start", config: {} });
    expect(workflowAdmission(workflow).allowed).toBe(false);
    expect(workflowAdmission(workflow).reason).toContain("2 core.Start nodes");
  });

  it("generates unique ids", () => {
    const workflow = emptyWorkflow();
    expect(nextNodeId(descriptor("windows.Input.Keyboard"), workflow.nodes)).toBe("keyboard");
    workflow.nodes.push({ id: "keyboard", type: "windows.Input.Keyboard", config: {} });
    const second = nextNodeId(descriptor("windows.Input.Keyboard"), workflow.nodes);
    expect(second).not.toBe("keyboard");

    expect(nextEdgeId("a", "b", [])).toBe("a-b");
    const existing: WorkflowEdge[] = [{ id: "a-b", kind: "control", source: "a", target: "b" }];
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

  it("checks port value compatibility conservatively", () => {
    expect(valueTypesCompatible("any", "image")).toBe(true);
    expect(valueTypesCompatible("image", "any")).toBe(true);
    expect(valueTypesCompatible("number", "number")).toBe(true);
    expect(valueTypesCompatible("path", "string")).toBe(true);
    expect(valueTypesCompatible("number", "string")).toBe(false);
    expect(valueTypesCompatible("image", "object")).toBe(false);
  });

  it("detects duplicate connections but distinguishes ports", () => {
    const edges: WorkflowEdge[] = [
      { id: "e1", kind: "control", source: "a", target: "b" },
      { id: "e2", kind: "data", source: "a", target: "c", source_port: "other", target_port: "in" },
    ];
    expect(edgeExists(edges, "a", "b")).toBe(true);
    expect(edgeExists(edges, "a", "b", "out", "in")).toBe(true);
    expect(edgeExists(edges, "a", "c")).toBe(false);
    expect(edgeExists(edges, "a", "c", "other", "in", "data")).toBe(true);
  });

  it("reports multiple Start nodes as a local error", () => {
    const workflow = emptyWorkflow();
    workflow.nodes.push({
      id: "start2",
      type: "core.Start",
      label: "Second Start",
      config: {},
    });
    expect(localProblems(workflow).some((line) => line.includes("2 core.Start nodes"))).toBe(true);
  });

  it("reports the mistakes an editor can catch locally", () => {
    const workflow: Workflow = {
      schema_version: "2.1",
      id: "wf.bad",
      metadata: { name: "bad", tags: [] },
      nodes: [
        { id: "start", type: "core.Start", config: {} },
        { id: "start", type: "core.Log", config: {} },
      ],
      edges: [{ id: "e1", kind: "control", source: "start", target: "ghost" }],
      variables: {},
    };
    const problems = localProblems(workflow);
    expect(problems.some((line) => line.includes("duplicate node id"))).toBe(true);
    expect(problems.some((line) => line.includes("missing target"))).toBe(true);
    expect(problems.some((line) => line.includes("no core.End"))).toBe(true);
  });

  it("flags a directed cycle instead of waiting for the runtime", () => {
    const workflow: Workflow = {
      schema_version: "2.1",
      id: "wf.cycle",
      metadata: { name: "cycle", tags: [] },
      nodes: [
        { id: "start", type: "core.Start", config: {} },
        { id: "a", type: "core.Log", config: {} },
        { id: "b", type: "core.Log", config: {} },
        { id: "end", type: "core.End", config: {} },
      ],
      edges: [
        { id: "e1", kind: "control", source: "start", target: "a" },
        { id: "e2", kind: "control", source: "a", target: "b" },
        { id: "e3", kind: "control", source: "b", target: "a" },
      ],
      variables: {},
    };
    const problems = localProblems(workflow);
    expect(problems.some((line) => line.includes("cycle") && line.includes("`a`"))).toBe(true);
  });

  it("accepts a diamond-shaped acyclic graph", () => {
    const workflow: Workflow = {
      schema_version: "2.1",
      id: "wf.diamond",
      metadata: { name: "diamond", tags: [] },
      nodes: [
        { id: "start", type: "core.Start", config: {} },
        { id: "a", type: "core.Log", config: {} },
        { id: "b", type: "core.Log", config: {} },
        { id: "end", type: "core.End", config: {} },
      ],
      edges: [
        { id: "e1", kind: "control", source: "start", target: "a" },
        { id: "e2", kind: "control", source: "start", target: "b" },
        { id: "e3", kind: "control", source: "a", target: "end" },
        { id: "e4", kind: "control", source: "b", target: "end" },
      ],
      variables: {},
    };
    expect(localProblems(workflow)).toEqual([]);
  });

  it("migrates legacy edges to explicit control and reports removed data mappings", () => {
    const legacy: Partial<Workflow> = {
      schema_version: "2.0",
      id: "wf.legacy",
      metadata: { name: "Legacy", tags: [] },
      nodes: [
        { id: "start", type: "core.Start", config: {} },
        { id: "end", type: "core.End", config: {} },
      ],
      edges: [
        {
          id: "e1",
          source: "start",
          target: "end",
          source_port: "out",
          target_port: "in",
        } as WorkflowEdge,
      ],
      variables: {},
    };

    const { workflow, notes } = migrateLegacyWorkflow(legacy);
    expect(workflow.schema_version).toBe("2.1");
    expect(workflow.edges?.[0]).toMatchObject({
      id: "e1",
      kind: "control",
      source: "start",
      target: "end",
    });
    expect(workflow.edges?.[0].source_port).toBeUndefined();
    expect(notes).toHaveLength(1);
  });});
