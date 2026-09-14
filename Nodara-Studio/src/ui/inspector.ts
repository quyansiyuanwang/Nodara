/**
 * The properties panel.
 *
 * Configuration forms are rendered from the node descriptor's JSON Schema, so a
 * plugin's configuration UI appears the moment the plugin is installed.
 */

import { localizeDiagnostic, t } from "../i18n";
import { renderConfigForm } from "../schema/form";
import {
  Diagnostic,
  NodeDescriptor,
  Workflow,
  WorkflowEdge,
  WorkflowNode,
} from "../runtime/types";

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

  /** Render the selected node or connection, plus relevant diagnostics. */
  render(nodeId: string | null, diagnostics: Diagnostic[] = [], edgeId: string | null = null): void {
    this.root.replaceChildren();
    if (edgeId) {
      const edge = this.workflow.edges.find((candidate) => candidate.id === edgeId);
      if (!edge) {
        this.root.appendChild(muted(t("inspector.nodeMissing")));
        return;
      }
      this.renderEdge(edge);
      return;
    }
    if (!nodeId) {
      this.renderWorkflow();
      this.renderVariables(diagnostics);
      return;
    }
    const node = this.workflow.nodes.find((candidate) => candidate.id === nodeId);
    if (!node) {
      this.root.appendChild(muted(t("inspector.nodeMissing")));
      return;
    }

    const descriptor = this.descriptorFor(node.type);
    this.renderHeader(node, descriptor);
    this.renderIdentity(node);
    this.renderExecution(node);
    this.renderConfig(node, descriptor);
    this.renderDiagnostics(diagnostics.filter((item) => item.node_id === node.id));
  }

  private renderWorkflow(): void {
    const heading = document.createElement("h4");
    heading.className = "inspector__section";
    heading.textContent = t("inspector.workflow");
    this.root.appendChild(heading);

    const addField = (
      key: string,
      title: string,
      value: string,
      multiline = false,
    ): HTMLInputElement | HTMLTextAreaElement => {
      const field = document.createElement("div");
      field.className = "field";
      const label = document.createElement("label");
      label.className = "field__label";
      label.htmlFor = key;
      label.textContent = title;
      const input = document.createElement(multiline ? "textarea" : "input") as
        | HTMLInputElement
        | HTMLTextAreaElement;
      input.id = key;
      input.className = "input";
      input.value = value;
      if (multiline) (input as HTMLTextAreaElement).rows = 2;
      input.addEventListener("input", () => {
        if (key === "workflow-id") this.workflow.id = input.value.trim();
        else if (key === "workflow-name") this.workflow.metadata.name = input.value;
        else if (key === "workflow-description") {
          this.workflow.metadata.description = input.value || undefined;
        } else if (key === "workflow-tags") {
          this.workflow.metadata.tags = input.value
            .split(",")
            .map((tag) => tag.trim())
            .filter(Boolean);
        }
        this.handlers.onChange();
      });
      field.append(label, input);
      this.root.appendChild(field);
      return input;
    };

    addField("workflow-id", t("inspector.workflowId"), this.workflow.id);
    addField("workflow-name", t("inspector.workflowName"), this.workflow.metadata.name);
    addField(
      "workflow-description",
      t("inspector.workflowDescription"),
      this.workflow.metadata.description ?? "",
      true,
    );
    const tags = addField(
      "workflow-tags",
      t("inspector.workflowTags"),
      this.workflow.metadata.tags.join(", "),
    );
    tags.title = t("inspector.workflowTagsHint");
  }

  private renderEdge(edge: WorkflowEdge): void {
    const title = document.createElement("h3");
    title.className = "inspector__title";
    title.textContent = t("inspector.connectionTitle");
    this.root.appendChild(title);

    const path = document.createElement("p");
    path.className = "muted";
    path.textContent = t("inspector.edgePath", { source: edge.source, target: edge.target });
    this.root.appendChild(path);

    const ports = document.createElement("p");
    ports.className = "muted";
    ports.textContent = `${t("inspector.edgePorts")}: ${t("inspector.edgePortsValue", {
      source: edge.source,
      sourcePort: edge.source_port ?? "out",
      target: edge.target,
      targetPort: edge.target_port ?? "in",
    })}`;
    this.root.appendChild(ports);

    const labelField = document.createElement("div");
    labelField.className = "field";
    const label = document.createElement("label");
    label.className = "field__label";
    label.textContent = t("inspector.edgeLabel");
    label.title = t("inspector.edgeLabelDetail");
    const labelInput = document.createElement("input");
    labelInput.className = "input";
    labelInput.value = edge.label ?? "";
    labelInput.placeholder = t("inspector.edgeLabelDetail");
    labelInput.addEventListener("input", () => {
      const value = labelInput.value.trim();
      if (value) edge.label = value;
      else delete edge.label;
      this.handlers.onChange();
    });
    labelField.append(label, labelInput);
    this.root.appendChild(labelField);

    const conditionField = document.createElement("div");
    conditionField.className = "field";
    const conditionLabel = document.createElement("label");
    conditionLabel.className = "field__label";
    conditionLabel.textContent = t("inspector.edgeCondition");
    conditionLabel.title = t("inspector.edgeConditionDetail");
    const condition = document.createElement("textarea");
    condition.className = "input input--code";
    condition.rows = 3;
    condition.spellcheck = false;
    condition.placeholder = t("inspector.edgeConditionPlaceholder");
    condition.value = edge.condition ?? "";
    condition.addEventListener("input", () => {
      const value = condition.value.trim();
      if (value) edge.condition = value;
      else delete edge.condition;
      this.handlers.onChange();
    });
    const conditionHint = document.createElement("p");
    conditionHint.className = "field__hint";
    conditionHint.textContent = t("inspector.edgeConditionDetail");
    conditionField.append(conditionLabel, condition, conditionHint);
    this.root.appendChild(conditionField);
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
      gate.textContent = t("inspector.gated", { permissions });
      this.root.appendChild(gate);
    }
  }

  private renderIdentity(node: WorkflowNode): void {
    const identity = document.createElement("div");
    identity.className = "field";

    const idLabel = document.createElement("label");
    idLabel.className = "field__label";
    idLabel.textContent = t("inspector.nodeId");
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
    labelLabel.textContent = t("inspector.label");
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

  private renderExecution(node: WorkflowNode): void {
    const heading = document.createElement("h4");
    heading.className = "inspector__section";
    heading.textContent = t("inspector.execution");
    this.root.appendChild(heading);

    renderConfigForm(
      this.root,
      {
        type: "object",
        properties: {
          enabled: {
            type: "boolean",
            title: t("execution.enabled"),
            description: t("execution.enabledDetail"),
            default: true,
          },
          condition: {
            type: "string",
            title: t("execution.condition"),
            description: t("execution.conditionDetail"),
          },
          delay_before_ms: {
            type: "integer",
            title: t("execution.delayBefore"),
            description: t("execution.delayBeforeDetail"),
            minimum: 0,
            default: 0,
          },
          delay_after_ms: {
            type: "integer",
            title: t("execution.delayAfter"),
            description: t("execution.delayAfterDetail"),
            minimum: 0,
            default: 0,
          },
          continue_on_error: {
            type: "boolean",
            title: t("execution.continueOnError"),
            description: t("execution.continueOnErrorDetail"),
            default: false,
          },
          retry: {
            type: "integer",
            title: t("execution.retries"),
            description: t("execution.retriesDetail"),
            minimum: 0,
            default: 0,
          },
          retry_delay_ms: {
            type: "integer",
            title: t("execution.retryDelay"),
            description: t("execution.retryDelayDetail"),
            minimum: 0,
            default: 0,
          },
        },
      },
      {
        enabled: node.enabled ?? true,
        condition: node.condition ?? "",
        delay_before_ms: node.delay_before_ms ?? 0,
        delay_after_ms: node.delay_after_ms ?? 0,
        continue_on_error: node.continue_on_error ?? false,
        retry: node.retry ?? 0,
        retry_delay_ms: node.retry_delay_ms ?? 0,
      },
      (key, value) => this.updateExecution(node, key, value),
    );
  }

  private updateExecution(node: WorkflowNode, key: string, value: unknown): void {
    const number = typeof value === "number" && Number.isFinite(value) ? Math.max(0, value) : 0;
    switch (key) {
      case "enabled":
        if (value === false) node.enabled = false;
        else delete node.enabled;
        break;
      case "condition": {
        const next = typeof value === "string" ? value.trim() : "";
        if (next) node.condition = next;
        else delete node.condition;
        break;
      }
      case "delay_before_ms":
        if (number > 0) node.delay_before_ms = number;
        else delete node.delay_before_ms;
        break;
      case "delay_after_ms":
        if (number > 0) node.delay_after_ms = number;
        else delete node.delay_after_ms;
        break;
      case "continue_on_error":
        if (value === true) node.continue_on_error = true;
        else delete node.continue_on_error;
        break;
      case "retry":
        if (number > 0) node.retry = Math.floor(number);
        else delete node.retry;
        break;
      case "retry_delay_ms":
        if (number > 0) node.retry_delay_ms = number;
        else delete node.retry_delay_ms;
        break;
      default:
        return;
    }
    this.handlers.onChange();
  }

  private renderConfig(node: WorkflowNode, descriptor: NodeDescriptor | undefined): void {
    const heading = document.createElement("h4");
    heading.className = "inspector__section";
    heading.textContent = t("inspector.configuration");
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
    const headingRow = document.createElement("div");
    headingRow.className = "section-header";
    const heading = document.createElement("h4");
    heading.className = "inspector__section";
    heading.textContent = t("inspector.variables");
    const add = document.createElement("button");
    add.type = "button";
    add.className = "btn btn--small";
    add.textContent = t("actions.addVariable");
    add.addEventListener("click", () => {
      let index = 1;
      while (this.workflow.variables[`variable${index}`]) index += 1;
      this.workflow.variables[`variable${index}`] = { value: "", secret: false };
      this.handlers.onChange();
      this.render(null, diagnostics);
    });
    headingRow.append(heading, add);
    this.root.appendChild(headingRow);

    const hint = document.createElement("p");
    hint.className = "muted";
    hint.textContent = t("inspector.variableHint");
    this.root.appendChild(hint);

    const names = Object.keys(this.workflow.variables);
    if (names.length === 0) {
      this.root.appendChild(muted(t("inspector.noVariables")));
    }
    for (const name of names) {
      const variable = this.workflow.variables[name];
      const card = document.createElement("div");
      card.className = "variable-card";
      card.dataset.variableName = name;

      const header = document.createElement("div");
      header.className = "variable-card__header";
      const label = document.createElement("code");
      label.textContent = `{{${name}}}`;
      const remove = document.createElement("button");
      remove.type = "button";
      remove.className = "btn btn--small";
      remove.textContent = t("actions.deleteVariable");
      remove.addEventListener("click", () => {
        delete this.workflow.variables[name];
        this.handlers.onChange();
        this.render(null, diagnostics);
      });
      header.append(label, remove);

      const value = document.createElement("textarea");
      value.className = "input input--code";
      value.rows = 2;
      value.value =
        typeof variable.value === "string" ? variable.value : JSON.stringify(variable.value);
      value.addEventListener("change", () => {
        try {
          variable.value = JSON.parse(value.value);
        } catch {
          variable.value = value.value;
        }
        this.handlers.onChange();
      });

      const description = document.createElement("input");
      description.className = "input";
      description.placeholder = t("inspector.variableDescription");
      description.value = variable.description ?? "";
      description.addEventListener("input", () => {
        variable.description = description.value || undefined;
        this.handlers.onChange();
      });

      const secretLabel = document.createElement("label");
      secretLabel.className = "field__check";
      const secret = document.createElement("input");
      secret.type = "checkbox";
      secret.checked = variable.secret === true;
      secret.addEventListener("change", () => {
        variable.secret = secret.checked;
        this.handlers.onChange();
      });
      secretLabel.append(secret, document.createTextNode(` ${t("inspector.secret")}`));

      card.append(header, value, description, secretLabel);
      this.root.appendChild(card);
    }
    this.renderDiagnostics(diagnostics);
  }

  private renderDiagnostics(diagnostics: Diagnostic[]): void {
    if (diagnostics.length === 0) return;
    const heading = document.createElement("h4");
    heading.className = "inspector__section";
    heading.textContent = t("inspector.problems");
    this.root.appendChild(heading);
    for (const raw of diagnostics) {
      const diagnostic = localizeDiagnostic(raw);
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
