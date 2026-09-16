/**
 * The properties panel.
 *
 * Configuration forms are rendered from the node descriptor's JSON Schema, so a
 * plugin's configuration UI appears the moment the plugin is installed.
 */

import { localizeDiagnostic, t } from "../i18n";
import { renderConfigForm } from "../schema/form";
import {
  EDGE_COLORS,
  edgeColor,
  NODE_COLORS,
  nodeColor,
  renameNodeVisual,
  setEdgeColor,
  setNodeColor,
} from "../model/visuals";
import {
  assignNodesToGroup,
  createWorkflowGroup,
  deleteWorkflowGroup,
  getWorkflowGroups,
  groupForNode,
  GROUP_COLORS,
  removeNodesFromGroups,
  renameNodeInGroups,
  renameWorkflowGroup,
  setWorkflowGroupColor,
} from "../model/groups";
import type { ScreenRegion } from "./region-picker";
import {
  Diagnostic,
  NodeDescriptor,
  Workflow,
  WorkflowEdge,
  WorkflowNode,
} from "../runtime/types";

export interface InspectorHandlers {
  onChange: () => void;
  getRunOverride?: (name: string) => unknown;
  setRunOverride?: (name: string, value: unknown) => void;
  clearRunOverride?: (name: string) => void;
  pickCaptureRegion?: () => Promise<ScreenRegion | null>;
  onSelectNodes?: (nodeIds: string[]) => void;
}

export class Inspector {
  constructor(
    private readonly root: HTMLElement,
    private readonly workflow: Workflow,
    private readonly descriptorFor: (nodeType: string) => NodeDescriptor | undefined,
    private readonly handlers: InspectorHandlers,
  ) {}

  private section(titleKey: string, key: string, defaultOpen = true): HTMLElement {
    const details = document.createElement("details");
    details.className = "inspector__fold";
    details.dataset.section = key;
    let open = defaultOpen;
    try {
      const stored = localStorage.getItem(`nodara.inspector.${key}.open`);
      if (stored !== null) open = stored === "true";
    } catch {
      // Storage is optional.
    }
    details.open = open;
    details.addEventListener("toggle", () => {
      try {
        localStorage.setItem(`nodara.inspector.${key}.open`, String(details.open));
      } catch {
        // The current session still keeps the selected state.
      }
    });
    const summary = document.createElement("summary");
    summary.textContent = t(titleKey);
    const body = document.createElement("div");
    body.className = "inspector__fold-body";
    details.append(summary, body);
    this.root.appendChild(details);
    return body;
  }

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
      this.renderGroups();
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
    this.renderNodeAppearance(node);
    this.renderExecution(node, descriptor);
    this.renderNodeGroup(node);
    this.renderConfig(node, descriptor);
    this.renderDiagnostics(diagnostics.filter((item) => item.node_id === node.id));
  }

  /** Render bulk execution settings and group management for a multi-selection. */
  renderSelection(nodeIds: string[], diagnostics: Diagnostic[] = []): void {
    this.root.replaceChildren();
    const nodes = this.workflow.nodes.filter((node) => nodeIds.includes(node.id));
    if (nodes.length === 0) {
      this.root.appendChild(muted(t("inspector.nodeMissing")));
      return;
    }

    const title = document.createElement("h3");
    title.className = "inspector__title";
    title.textContent = t("inspector.selectionTitle", { count: nodes.length });
    this.root.appendChild(title);

    const subtitle = document.createElement("p");
    subtitle.className = "muted";
    subtitle.textContent = t("inspector.selectionSubtitle");
    this.root.appendChild(subtitle);

    this.renderGroupMembership(nodes);
    this.renderBulkExecution(nodes);
    const relevant = diagnostics.filter(
      (item) => item.node_id !== undefined && nodeIds.includes(item.node_id),
    );
    this.renderDiagnostics(relevant);
  }

  /** Persistent group manager shown when the canvas has no active selection. */
  private renderGroups(): void {
    const content = this.section("inspector.groups", "groups", true);
    const hint = document.createElement("p");
    hint.className = "field__hint";
    hint.textContent = t("groups.workflowHint");
    content.appendChild(hint);

    const groups = getWorkflowGroups(this.workflow);
    if (groups.length === 0) {
      content.appendChild(muted(t("groups.empty")));
      return;
    }

    const list = document.createElement("div");
    list.className = "group-manager";
    for (const group of groups) {
      const card = document.createElement("div");
      card.className = "group-manager__card";
      card.dataset.groupId = group.id;
      card.style.setProperty("--group-color", group.color);

      const heading = document.createElement("div");
      heading.className = "group-manager__heading";
      const select = document.createElement("button");
      select.type = "button";
      select.className = "group-manager__select";
      select.textContent = group.name;
      select.title = t("groups.selectAll");
      select.addEventListener("click", () => this.handlers.onSelectNodes?.(group.node_ids));
      const count = document.createElement("span");
      count.className = "badge";
      count.textContent = t("groups.nodeCount", { count: group.node_ids.length });
      heading.append(select, count);
      card.appendChild(heading);

      const name = document.createElement("input");
      name.className = "input";
      name.value = group.name;
      name.setAttribute("aria-label", t("groups.name"));
      name.addEventListener("change", () => {
        renameWorkflowGroup(this.workflow, group.id, name.value);
        this.handlers.onChange();
      });
      card.appendChild(name);

      const actions = document.createElement("div");
      actions.className = "group-manager__actions";
      const colors = document.createElement("div");
      colors.className = "group-colors";
      for (const color of GROUP_COLORS) {
        const swatch = document.createElement("button");
        swatch.type = "button";
        swatch.className = "group-color";
        swatch.style.setProperty("--group-color", color);
        swatch.classList.toggle("group-color--active", color === group.color);
        swatch.setAttribute("aria-label", color);
        swatch.addEventListener("click", () => {
          setWorkflowGroupColor(this.workflow, group.id, color);
          this.handlers.onChange();
        });
        colors.appendChild(swatch);
      }
      const remove = document.createElement("button");
      remove.type = "button";
      remove.className = "btn btn--small";
      remove.textContent = t("groups.removeGroup");
      remove.addEventListener("click", () => {
        deleteWorkflowGroup(this.workflow, group.id);
        this.handlers.onChange();
      });
      actions.append(colors, remove);
      card.append(name, actions);
      list.appendChild(card);
    }
    content.appendChild(list);
  }

  private renderNodeGroup(node: WorkflowNode): void {
    this.renderGroupMembership([node]);
  }

  private renderGroupMembership(nodes: WorkflowNode[]): void {
    const content = this.section("inspector.groupMembership", "group-membership", true);
    const groups = getWorkflowGroups(this.workflow);
    const memberships = nodes.map((node) => groupForNode(this.workflow, node.id)?.id ?? null);
    const commonId = memberships.every((id) => id === memberships[0]) ? memberships[0] : null;
    const nodeIds = nodes.map((node) => node.id);

    const assignment = document.createElement("div");
    assignment.className = "group-assignment";
    const select = document.createElement("select");
    select.className = "input";
    select.setAttribute("aria-label", t("groups.assignment"));
    const none = document.createElement("option");
    none.value = "";
    none.textContent = t("groups.none");
    select.appendChild(none);
    for (const group of groups) {
      const option = document.createElement("option");
      option.value = group.id;
      option.textContent = group.name;
      select.appendChild(option);
    }
    select.value = commonId ?? "";
    select.addEventListener("change", () => {
      if (select.value) assignNodesToGroup(this.workflow, select.value, nodeIds);
      else removeNodesFromGroups(this.workflow, nodeIds);
      this.handlers.onChange();
    });

    const createRow = document.createElement("div");
    createRow.className = "group-assignment__create";
    const name = document.createElement("input");
    name.className = "input";
    name.placeholder = t("groups.newName");
    const create = document.createElement("button");
    create.type = "button";
    create.className = "btn btn--small";
    create.textContent = t("groups.create");
    create.disabled = nodes.length === 0;
    create.addEventListener("click", () => {
      const group = createWorkflowGroup(
        this.workflow,
        nodeIds,
        name.value.trim() || t("groups.defaultName", { index: groups.length + 1 }),
      );
      this.handlers.onChange();
      this.handlers.onSelectNodes?.(group.node_ids);
    });
    createRow.append(name, create);

    assignment.append(select, createRow);
    content.appendChild(assignment);

    if (commonId) {
      const group = groups.find((candidate) => candidate.id === commonId);
      if (group) {
        const actions = document.createElement("div");
        actions.className = "group-assignment__actions";
        const color = document.createElement("div");
        color.className = "group-colors";
        for (const value of GROUP_COLORS) {
          const swatch = document.createElement("button");
          swatch.type = "button";
          swatch.className = "group-color";
          swatch.style.setProperty("--group-color", value);
          swatch.classList.toggle("group-color--active", value === group.color);
          swatch.setAttribute("aria-label", value);
          swatch.addEventListener("click", () => {
            setWorkflowGroupColor(this.workflow, group.id, value);
            this.handlers.onChange();
          });
          color.appendChild(swatch);
        }
        const remove = document.createElement("button");
        remove.type = "button";
        remove.className = "btn btn--small";
        remove.textContent = t("groups.removeMembers");
        remove.addEventListener("click", () => {
          removeNodesFromGroups(this.workflow, nodeIds);
          this.handlers.onChange();
        });
        actions.append(color, remove);
        content.appendChild(actions);
      }
    }
  }

  private renderBulkExecution(nodes: WorkflowNode[]): void {
    const content = this.section("inspector.bulkExecution", "bulk-execution", true);
    const hint = document.createElement("p");
    hint.className = "field__hint";
    hint.textContent = t("inspector.bulkExecutionHint");
    content.appendChild(hint);

    const booleanField = (
      labelKey: string,
      read: (node: WorkflowNode) => boolean,
      apply: (node: WorkflowNode, value: boolean) => void,
    ) => {
      const values = nodes.map(read);
      const mixed = values.some((value) => value !== values[0]);
      const label = document.createElement("label");
      label.className = "group-bulk-field group-bulk-field--check";
      const input = document.createElement("input");
      input.type = "checkbox";
      input.dataset.field = labelKey;
      input.checked = !mixed && values[0];
      input.indeterminate = mixed;
      input.addEventListener("change", () => {
        for (const node of nodes) apply(node, input.checked);
        this.handlers.onChange();
      });
      const text = document.createElement("span");
      text.textContent = mixed ? t("canvas.multipleValues") : t(labelKey);
      label.append(input, text);
      content.appendChild(label);
    };

    const numberField = (
      labelKey: string,
      read: (node: WorkflowNode) => number,
      apply: (node: WorkflowNode, value: number) => void,
    ) => {
      const values = nodes.map(read);
      const mixed = values.some((value) => value !== values[0]);
      const label = document.createElement("label");
      label.className = "group-bulk-field";
      const text = document.createElement("span");
      text.textContent = t(labelKey);
      const input = document.createElement("input");
      input.className = "input input--small";
      input.dataset.field = labelKey;
      input.type = "number";
      input.min = "0";
      input.value = mixed ? "" : String(values[0]);
      input.placeholder = mixed ? t("canvas.multipleValues") : "";
      input.addEventListener("change", () => {
        const value = Math.max(0, Number(input.value) || 0);
        for (const node of nodes) apply(node, value);
        this.handlers.onChange();
      });
      label.append(text, input);
      content.appendChild(label);
    };

    const textField = (
      labelKey: string,
      read: (node: WorkflowNode) => string,
      apply: (node: WorkflowNode, value: string) => void,
    ) => {
      const values = nodes.map(read);
      const mixed = values.some((value) => value !== values[0]);
      const label = document.createElement("label");
      label.className = "group-bulk-field";
      const text = document.createElement("span");
      text.textContent = t(labelKey);
      const input = document.createElement("input");
      input.className = "input input--small";
      input.dataset.field = labelKey;
      input.value = mixed ? "" : values[0];
      input.placeholder = mixed ? t("canvas.multipleValues") : "";
      input.addEventListener("change", () => {
        for (const node of nodes) apply(node, input.value);
        this.handlers.onChange();
      });
      label.append(text, input);
      content.appendChild(label);
    };

    booleanField(
      "execution.enabled",
      (node) => node.enabled ?? true,
      (node, value) => {
        if (value) delete node.enabled;
        else node.enabled = false;
      },
    );
    booleanField(
      "execution.breakpoint",
      (node) => node.breakpoint ?? false,
      (node, value) => {
        if (value) node.breakpoint = true;
        else delete node.breakpoint;
      },
    );
    textField(
      "execution.condition",
      (node) => node.condition ?? "",
      (node, value) => {
        const next = value.trim();
        if (next) node.condition = next;
        else delete node.condition;
      },
    );
    numberField(
      "execution.delayBefore",
      (node) => node.delay_before_ms ?? 0,
      (node, value) => {
        if (value > 0) node.delay_before_ms = value;
        else delete node.delay_before_ms;
      },
    );
    booleanField(
      "execution.continueOnError",
      (node) => node.continue_on_error ?? false,
      (node, value) => {
        if (value) node.continue_on_error = true;
        else delete node.continue_on_error;
      },
    );
    numberField(
      "execution.retries",
      (node) => node.retry ?? 0,
      (node, value) => {
        if (value > 0) node.retry = Math.floor(value);
        else delete node.retry;
      },
    );
    numberField(
      "execution.timeout",
      (node) => node.timeout_ms ?? 0,
      (node, value) => {
        if (value > 0) node.timeout_ms = value;
        else delete node.timeout_ms;
      },
    );
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
        } else if (key === "workflow-author") {
          this.workflow.metadata.author = input.value || undefined;
        } else if (key === "workflow-version") {
          this.workflow.metadata.version = input.value || undefined;
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
    addField(
      "workflow-author",
      t("inspector.workflowAuthor"),
      this.workflow.metadata.author ?? "",
    );
    addField(
      "workflow-version",
      t("inspector.workflowVersion"),
      this.workflow.metadata.version ?? "",
    );
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
    this.renderEdgeAppearance(edge);

    if (edge.kind === "data") {
      const ports = document.createElement("p");
      ports.className = "muted";
      ports.textContent = `${t("inspector.edgePorts")}: ${t("inspector.edgePortsValue", {
        source: edge.source,
        sourcePort: edge.source_port ?? "out",
        target: edge.target,
        targetPort: edge.target_port ?? "in",
      })}`;
      this.root.appendChild(ports);
      return;
    }

    const branchField = document.createElement("div");
    branchField.className = "field";
    const branchLabel = document.createElement("label");
    branchLabel.className = "field__label";
    branchLabel.textContent = t("inspector.edgeBranch");
    branchLabel.title = t("inspector.edgeBranchDetail");
    const branch = document.createElement("select");
    branch.className = "input";
    for (const [value, key] of [
      ["always", "inspector.edgeBranchAlways"],
      ["success", "inspector.edgeBranchSuccess"],
      ["failure", "inspector.edgeBranchFailure"],
    ] as const) {
      const option = document.createElement("option");
      option.value = value;
      option.textContent = t(key);
      branch.appendChild(option);
    }
    branch.value = edge.branch ?? "always";
    branch.addEventListener("change", () => {
      const next = branch.value as NonNullable<WorkflowEdge["branch"]>;
      if (next === "always") delete edge.branch;
      else edge.branch = next;
      this.handlers.onChange();
    });
    const branchHint = document.createElement("p");
    branchHint.className = "field__hint";
    branchHint.textContent = t("inspector.edgeBranchDetail");
    branchField.append(branchLabel, branch, branchHint);
    this.root.appendChild(branchField);

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
      renameNodeInGroups(this.workflow, previous, next);
      renameNodeVisual(this.workflow, previous, next);
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

  private renderExecution(node: WorkflowNode, descriptor?: NodeDescriptor): void {
    const content = this.section("inspector.execution", "execution");

    renderConfigForm(
      content,
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
          breakpoint: {
            type: "boolean",
            title: t("execution.breakpoint"),
            description: t("execution.breakpointDetail"),
            default: false,
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
          ...(descriptor?.plugin_id
            ? {
                timeout_ms: {
                  type: "integer" as const,
                  title: t("execution.timeout"),
                  description: t("execution.timeoutDetail"),
                  minimum: 0,
                },
              }
            : {}),
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
          retry_backoff: {
            type: "string",
            title: t("execution.retryBackoff"),
            description: t("execution.retryBackoffDetail"),
            enum: ["fixed", "exponential"],
            default: "fixed",
          },
          retry_max_delay_ms: {
            type: "integer",
            title: t("execution.retryMaxDelay"),
            description: t("execution.retryMaxDelayDetail"),
            minimum: 0,
            default: 0,
          },
          result_var: {
            type: "string",
            title: t("execution.resultVar"),
            description: t("execution.resultVarDetail"),
          },
          result_port: {
            type: "string",
            title: t("execution.resultPort"),
            description: t("execution.resultPortDetail"),
          },
        },
      },
      {
        enabled: node.enabled ?? true,
        condition: node.condition ?? "",
        breakpoint: node.breakpoint ?? false,
        delay_before_ms: node.delay_before_ms ?? 0,
        delay_after_ms: node.delay_after_ms ?? 0,
        continue_on_error: node.continue_on_error ?? false,
        timeout_ms: node.timeout_ms ?? 0,
        retry: node.retry ?? 0,
        retry_delay_ms: node.retry_delay_ms ?? 0,
        retry_backoff: node.retry_backoff ?? "fixed",
        retry_max_delay_ms: node.retry_max_delay_ms ?? 0,
        result_var: node.result_var ?? "",
        result_port: node.result_port ?? "",
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
      case "breakpoint":
        if (value === true) node.breakpoint = true;
        else delete node.breakpoint;
        break;
      case "delay_before_ms":
        if (number > 0) node.delay_before_ms = number;
        else delete node.delay_before_ms;
        break;
      case "delay_after_ms":
        if (number > 0) node.delay_after_ms = number;
        else delete node.delay_after_ms;
        break;
      case "timeout_ms":
        if (number > 0) node.timeout_ms = number;
        else delete node.timeout_ms;
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
      case "retry_backoff":
        if (value === "exponential") node.retry_backoff = "exponential";
        else delete node.retry_backoff;
        break;
      case "retry_max_delay_ms":
        if (number > 0) node.retry_max_delay_ms = number;
        else delete node.retry_max_delay_ms;
        break;
      case "result_var": {
        const next = typeof value === "string" ? value.trim() : "";
        if (next) node.result_var = next;
        else delete node.result_var;
        break;
      }
      case "result_port": {
        const next = typeof value === "string" ? value.trim() : "";
        if (next) node.result_port = next;
        else delete node.result_port;
        break;
      }
      default:
        return;
    }
    this.handlers.onChange();
  }

  private renderNodeAppearance(node: WorkflowNode): void {
    this.renderAppearance(
      t("inspector.appearance"),
      nodeColor(this.workflow, node.id),
      NODE_COLORS,
      (color) => {
        setNodeColor(this.workflow, node.id, color);
        this.handlers.onChange();
        this.render(node.id);
      },
    );
  }

  private renderEdgeAppearance(edge: WorkflowEdge): void {
    this.renderAppearance(
      t("inspector.appearance"),
      edgeColor(this.workflow, edge.id),
      EDGE_COLORS,
      (color) => {
        setEdgeColor(this.workflow, edge.id, color);
        this.handlers.onChange();
        this.render(null, [], edge.id);
      },
    );
  }

  private renderAppearance(
    title: string,
    current: string | undefined,
    palette: readonly string[],
    apply: (color: string | null) => void,
  ): void {
    const field = document.createElement("div");
    field.className = "field appearance-field";
    const label = document.createElement("label");
    label.className = "field__label";
    label.textContent = title;
    const swatches = document.createElement("div");
    swatches.className = "appearance-swatches";
    for (const color of palette) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "appearance-swatch";
      button.style.setProperty("--swatch", color);
      button.title = color;
      button.setAttribute("aria-label", `${title}: ${color}`);
      if (current?.toLowerCase() === color.toLowerCase()) button.classList.add("is-selected");
      button.addEventListener("click", () => apply(color));
      swatches.appendChild(button);
    }
    const reset = document.createElement("button");
    reset.type = "button";
    reset.className = "btn btn--small";
    reset.textContent = t("inspector.resetAppearance");
    reset.disabled = !current;
    reset.addEventListener("click", () => apply(null));
    field.append(label, swatches, reset);
    this.root.appendChild(field);
  }

  private renderConfig(node: WorkflowNode, descriptor: NodeDescriptor | undefined): void {
    const content = this.section("inspector.configuration", "configuration");

    node.config ??= {};
    const properties = descriptor?.config_schema?.properties ?? {};
    if (
      this.handlers.pickCaptureRegion &&
      descriptor?.outputs.some((port) => port.value_type === "image") &&
      ["x", "y", "width", "height"].every((key) => properties[key])
    ) {
      const toolbar = document.createElement("div");
      toolbar.className = "section-header";
      const hint = document.createElement("p");
      hint.className = "field__hint";
      hint.textContent = t("capture.pickerHint");
      const button = document.createElement("button");
      button.type = "button";
      button.className = "btn btn--small";
      button.textContent = t("capture.selectRegion");
      button.addEventListener("click", async () => {
        button.disabled = true;
        button.textContent = t("capture.capturing");
        try {
          const region = await this.handlers.pickCaptureRegion?.();
          if (region) {
            const config = node.config as Record<string, unknown>;
            for (const key of ["x", "y", "width", "height"] as const) {
              config[key] = region[key];
              const input = this.root.querySelector<HTMLInputElement>(`#field-${key}`);
              if (input) input.value = String(region[key]);
            }
            this.handlers.onChange();
          }
        } finally {
          button.disabled = false;
          button.textContent = t("capture.selectRegion");
        }
      });
      toolbar.append(hint, button);
      content.appendChild(toolbar);
    }
    renderConfigForm(
      content,
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
    const content = this.section("inspector.variables", "variables");
    const headingRow = document.createElement("div");
    headingRow.className = "section-header";
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
    headingRow.appendChild(add);
    content.appendChild(headingRow);

    const hint = document.createElement("p");
    hint.className = "muted";
    hint.textContent = t("inspector.variableHint");
    content.appendChild(hint);

    const names = Object.keys(this.workflow.variables);
    if (names.length === 0) {
      content.appendChild(muted(t("inspector.noVariables")));
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
        this.handlers.clearRunOverride?.(name);
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

      const override = this.handlers.getRunOverride?.(name);
      if (this.handlers.setRunOverride) {
        const overrideField = document.createElement("div");
        overrideField.className = "field";
        const overrideLabel = document.createElement("label");
        overrideLabel.className = "field__label";
        overrideLabel.textContent = t("inspector.runOverride");
        const overrideValue = document.createElement("textarea");
        overrideValue.className = "input input--code";
        overrideValue.rows = 2;
        overrideValue.placeholder = t("inspector.runOverrideHint");
        overrideValue.value =
          override === undefined
            ? ""
            : typeof override === "string"
              ? override
              : JSON.stringify(override);
        overrideValue.addEventListener("change", () => {
          let next: unknown = overrideValue.value;
          try {
            next = JSON.parse(overrideValue.value);
          } catch {
            // Plain strings are a valid override.
          }
          this.handlers.setRunOverride?.(name, next);
        });
        const overrideHint = document.createElement("p");
        overrideHint.className = "field__hint";
        overrideHint.textContent = t("inspector.runOverrideHint");
        overrideField.append(overrideLabel, overrideValue, overrideHint);
        if (override !== undefined && this.handlers.clearRunOverride) {
          const clear = document.createElement("button");
          clear.type = "button";
          clear.className = "btn btn--small";
          clear.textContent = t("actions.clearOverride");
          clear.addEventListener("click", () => {
            this.handlers.clearRunOverride?.(name);
            overrideValue.value = "";
          });
          overrideField.appendChild(clear);
        }
        card.appendChild(overrideField);
      }

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
      content.appendChild(card);
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
