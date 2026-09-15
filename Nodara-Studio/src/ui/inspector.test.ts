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

  it("edits workflow metadata and adds variables", () => {
    const workflow = emptyWorkflow();
    const root = document.createElement("div");
    document.body.appendChild(root);
    let changes = 0;
    const inspector = new Inspector(
      root,
      workflow,
      () => undefined,
      { onChange: () => { changes += 1; } },
    );

    inspector.render(null);
    const id = root.querySelector<HTMLInputElement>("#workflow-id")!;
    id.value = "workflow.test";
    id.dispatchEvent(new Event("input"));
    const name = root.querySelector<HTMLInputElement>("#workflow-name")!;
    name.value = "Test flow";
    name.dispatchEvent(new Event("input"));
    const tags = root.querySelector<HTMLInputElement>("#workflow-tags")!;
    tags.value = "smoke, windows";
    tags.dispatchEvent(new Event("input"));

    expect(workflow.id).toBe("workflow.test");
    expect(workflow.metadata.name).toBe("Test flow");
    expect(workflow.metadata.tags).toEqual(["smoke", "windows"]);

    const add = [...root.querySelectorAll<HTMLButtonElement>("button")].find(
      (button) => button.textContent?.includes("Add variable"),
    )!;
    add.click();
    expect(workflow.variables.variable1).toEqual({ value: "", secret: false });
    expect(changes).toBe(4);
  });

  it("edits run overrides without changing workflow defaults", () => {
    const workflow = emptyWorkflow();
    workflow.variables.token = { value: "default", secret: false };
    const overrides = new Map<string, unknown>();
    const root = document.createElement("div");
    document.body.appendChild(root);
    const inspector = new Inspector(
      root,
      workflow,
      () => undefined,
      {
        onChange: () => undefined,
        getRunOverride: (name) => overrides.get(name),
        setRunOverride: (name, value) => overrides.set(name, value),
        clearRunOverride: (name) => overrides.delete(name),
      },
    );
    inspector.render(null);

    const label = [...root.querySelectorAll<HTMLLabelElement>(".field__label")].find(
      (item) => item.textContent === "Run value override",
    )!;
    const input = label.closest(".field")!.querySelector("textarea")!;
    input.value = "override";
    input.dispatchEvent(new Event("change"));

    expect(overrides.get("token")).toBe("override");
    expect(workflow.variables.token.value).toBe("default");
  });

  it("edits the connection outcome branch", () => {
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
    const branch = root.querySelector<HTMLSelectElement>("select.input")!;
    expect(branch.value).toBe("always");
    branch.value = "failure";
    branch.dispatchEvent(new Event("change"));
    expect(workflow.edges[0].branch).toBe("failure");

    branch.value = "always";
    branch.dispatchEvent(new Event("change"));
    expect(workflow.edges[0].branch).toBeUndefined();
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

    const checkboxes = root.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
    checkboxes[0].checked = false;
    checkboxes[0].dispatchEvent(new Event("change"));
    checkboxes[1].checked = true;
    checkboxes[1].dispatchEvent(new Event("change"));
    expect(workflow.nodes.find((node) => node.id === "log")?.enabled).toBe(false);
    expect(workflow.nodes.find((node) => node.id === "log")?.continue_on_error).toBe(true);

    const labels = [...root.querySelectorAll<HTMLLabelElement>(".field__label")];
    const conditionLabel = labels.find((label) => label.textContent === "Run condition")!;
    const condition = conditionLabel.closest(".field")!.querySelector("textarea")!;
    condition.value = "allow";
    condition.dispatchEvent(new Event("input"));
    expect(workflow.nodes.find((node) => node.id === "log")?.condition).toBe("allow");

    const numbers = root.querySelectorAll<HTMLInputElement>('input[type="number"]');
    numbers[2].value = "3";
    numbers[2].dispatchEvent(new Event("input"));
    numbers[3].value = "25";
    numbers[3].dispatchEvent(new Event("input"));

    expect(workflow.nodes.find((node) => node.id === "log")?.retry).toBe(3);
    expect(workflow.nodes.find((node) => node.id === "log")?.retry_delay_ms).toBe(25);

    const resultVarLabel = labels.find((label) => label.textContent === "Store result as")!;
    const resultVar = resultVarLabel.closest(".field")!.querySelector("input")!;
    resultVar.value = "answer";
    resultVar.dispatchEvent(new Event("input"));
    const resultPortLabel = labels.find((label) => label.textContent === "Result port")!;
    const resultPort = resultPortLabel.closest(".field")!.querySelector("input")!;
    resultPort.value = "result";
    resultPort.dispatchEvent(new Event("input"));

    expect(workflow.nodes.find((node) => node.id === "log")?.result_var).toBe("answer");
    expect(workflow.nodes.find((node) => node.id === "log")?.result_port).toBe("result");
    expect(changes).toBe(7);
  });
});
