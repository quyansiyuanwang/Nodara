import { beforeEach, describe, expect, it } from "vitest";

import { Inspector } from "./inspector";
import { emptyWorkflow } from "../model/workflow";
import { NodeDescriptor } from "../runtime/types";

function descriptor(): NodeDescriptor {
  return {
    node_type: "core.Log",
    display_name: "Log",
    category: "Core",
    description: "Writes a message",
    version: "2.0.0",
    inputs: [],
    outputs: [],
    config_schema: {
      type: "object",
      properties: { message: { type: "string", title: "Message" } },
    },
    capabilities: [],
    permissions: [],
    dangerous: false,
    allows_additional_config: false,
  };
}

describe("node execution settings", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  it("edits connection labels and conditions directly", () => {
    const workflow = emptyWorkflow();
    workflow.edges.push({ id: "e1", source: "start", target: "end" });
    const root = document.createElement("div");
    document.body.appendChild(root);
    const inspector = new Inspector(
      root,
      workflow,
      () => undefined,
      { onChange: () => undefined },
    );

    inspector.render(null, [], "e1");
    const label = root.querySelector<HTMLInputElement>("input.input")!;
    label.value = "success";
    label.dispatchEvent(new Event("input"));
    const condition = root.querySelector<HTMLTextAreaElement>("textarea.input")!;
    condition.value = "score > 0.8";
    condition.dispatchEvent(new Event("input"));

    expect(workflow.edges[0].label).toBe("success");
    expect(workflow.edges[0].condition).toBe("score > 0.8");
  });

  it("edits common execution options directly in the inspector", () => {
    const workflow = emptyWorkflow();
    workflow.nodes.push({ id: "log", type: "core.Log", config: {} });
    let changes = 0;
    const root = document.createElement("div");
    document.body.appendChild(root);

    const inspector = new Inspector(
      root,
      workflow,
      (nodeType) => (nodeType === "core.Log" ? descriptor() : undefined),
      { onChange: () => { changes += 1; } },
    );
    inspector.render("log");

    const checkbox = root.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
    checkbox.checked = false;
    checkbox.dispatchEvent(new Event("change"));
    expect(workflow.nodes.find((node) => node.id === "log")?.enabled).toBe(false);

    const numbers = root.querySelectorAll<HTMLInputElement>('input[type="number"]');
    numbers[2].value = "3";
    numbers[2].dispatchEvent(new Event("input"));
    numbers[3].value = "25";
    numbers[3].dispatchEvent(new Event("input"));

    expect(workflow.nodes.find((node) => node.id === "log")?.retry).toBe(3);
    expect(workflow.nodes.find((node) => node.id === "log")?.retry_delay_ms).toBe(25);
    expect(changes).toBe(3);
  });
});
