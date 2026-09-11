/**
 * The properties panel.
 *
 * Configuration forms are rendered from the node descriptor's JSON Schema, so a
 * plugin's configuration UI appears the moment the plugin is installed.
 */

import { renderConfigForm } from "../schema/form";
import { Diagnostic, NodeDescriptor, Workflow, WorkflowNode } from "../runtime/types";

export interface InspectorHandlers {
  onChange: () => void;
}

export class Inspector {
  constructor(
    private readonly root: HTMLElement,
    private readonly workflow: Workflow,
    private readonly descriptorFor: (nodeType: string) => NodeDescriptor | undefined,
    private readonly handlers: InspectorHandlers,
  ) {}

  /** Render the selected node, plus any diagnostics that mention it. */
  render(nodeId: string | null, diagnostics: Diagnostic[] = []): void {
    this.root.replaceChildren();
    if (!nodeId) {
      this.root.appendChild(muted("Select a node to edit its configuration."));
      this.renderVariables(diagnostics);
      return;
    }
    const node = this.workflow.nodes.find((candidate) => candidate.id === nodeId);
    if (!node) {
      this.root.appendChild(muted("The selected node no longer exists."));
      return;
    }

    const descriptor = this.descriptorFor(node.type);
    this.renderHeader(node, descriptor);
    this.renderIdentity(node);
    this.renderConfig(node, descriptor);
    this.renderDiagnostics(diagnostics.filter((item) => item.node_id === node.id));
  }

  private renderHeader(node: WorkflowNode, descriptor: NodeDescriptor | undefined): void {
    const title = document.createElement("h3");
    title.className = "inspector__title";
    title.textContent = descriptor?.display_name ?? node.type;
    this.root.appendChild(title);

    const subtitle = document.createElement("p");
    subtitle.className = "muted";
    subtitle.textContent = descriptor?.description || node.type;
    this.root.appendChild(subtitle);

    if (descriptor?.dangerous || (descriptor?.permissions.length ?? 0) > 0) {
      const gate = document.createElement("p");
      gate.className = "gate";
      const permissions = descriptor?.permissions.join(", ") || "approval";
      gate.textContent = `Gated capability — the runtime will ask policy before running this node (${permissions}).`;
      this.root.appendChild(gate);
    }
  }

  private renderIdentity(node: WorkflowNode): void {
    const identity = document.createElement("div");
    identity.className = "field";

    const idLabel = document.createElement("label");
    idLabel.className = "field__label";
    idLabel.textContent = "Node id";
    const idInput = document.createElement("input");
    idInput.className = "input";
    idInput.value = node.id;
    idInput.addEventListener("change", () => {
      const previous = node.id;
      const next = idInput.value.trim();
      if (!next || this.workflow.nodes.some((other) => other.id === next)) {
        idInput.value = previous;
        return;
      }
      node.id = next;
      for (const edge of this.workflow.edges) {
        if (edge.source === previous) edge.source = next;
        if (edge.target === previous) edge.target = next;
      }
      this.handlers.onChange();
    });
    identity.appendChild(idLabel);
    identity.appendChild(idInput);

    const labelLabel = document.createElement("label");
    labelLabel.className = "field__label";
    labelLabel.textContent = "Label";
    const labelInput = document.createElement("input");
    labelInput.className = "input";
    labelInput.value = node.label ?? "";
    labelInput.addEventListener("input", () => {
      node.label = labelInput.value;
      this.handlers.onChange();
    });
    identity.appendChild(labelLabel);
    identity.appendChild(labelInput);

    this.root.appendChild(identity);
  }

  private renderConfig(node: WorkflowNode, descriptor: NodeDescriptor | undefined): void {
    const heading = document.createElement("h4");
    heading.className = "inspector__section";
    heading.textContent = "Configuration";
    this.root.appendChild(heading);

    node.config ??= {};
    renderConfigForm(
      this.root,
      descriptor?.config_schema ?? {},
      node.config as Record<string, unknown>,
      (key, value) => {
        const config = node.config as Record<string, unknown>;
        if (value === undefined) {
          delete config[key];
        } else {
          config[key] = value;
        }
        this.handlers.onChange();
      },
    );
  }

  private renderVariables(diagnostics: Diagnostic[]): void {
    const heading = document.createElement("h4");
    heading.className = "inspector__section";
    heading.textContent = "Workflow variables";
    this.root.appendChild(heading);

    const names = Object.keys(this.workflow.variables);
    if (names.length === 0) {
      this.root.appendChild(muted("No variables declared."));
    }
    for (const name of names) {
      const variable = this.workflow.variables[name];
      const row = document.createElement("div");
      row.className = "field";

      const label = document.createElement("label");
      label.className = "field__label";
      label.textContent = name;
      const input = document.createElement("input");
      input.className = "input";
      input.value =
        typeof variable.value === "string" ? variable.value : JSON.stringify(variable.value);
      input.addEventListener("change", () => {
        try {
          variable.value = JSON.parse(input.value);
        } catch {
          variable.value = input.value;
        }
        this.handlers.onChange();
      });

      row.appendChild(label);
      row.appendChild(input);
      this.root.appendChild(row);
    }
    this.renderDiagnostics(diagnostics);
  }

  private renderDiagnostics(diagnostics: Diagnostic[]): void {
    if (diagnostics.length === 0) return;
    const heading = document.createElement("h4");
    heading.className = "inspector__section";
    heading.textContent = "Problems";
    this.root.appendChild(heading);
    for (const diagnostic of diagnostics) {
      const item = document.createElement("p");
      item.className = `problem problem--${diagnostic.severity}`;
      item.textContent = `[${diagnostic.code}] ${diagnostic.message}`;
      this.root.appendChild(item);
    }
  }
}

function muted(text: string): HTMLElement {
  const element = document.createElement("p");
  element.className = "muted";
  element.textContent = text;
  return element;
}
