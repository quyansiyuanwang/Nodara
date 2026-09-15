import { t } from "../i18n";
import { Workflow, WorkflowVariable } from "../runtime/types";

export interface RunDialogHandlers {
  onRun: () => void;
  getOverride: (name: string) => unknown;
  setOverride: (name: string, value: unknown) => void;
  clearOverride: (name: string) => void;
}

type VariableKind = "string" | "number" | "boolean" | "json";

function variableKind(variable: WorkflowVariable): VariableKind {
  if (typeof variable.value === "boolean") return "boolean";
  if (typeof variable.value === "number") return "number";
  if (variable.value !== null && typeof variable.value === "object") return "json";
  return "string";
}

function formatValue(value: unknown): string {
  if (typeof value === "string") return value;
  if (value === undefined || value === null) return String(value);
  return JSON.stringify(value);
}

function fieldId(name: string): string {
  return `run-var-${name.replace(/[^A-Za-z0-9_-]/g, "-")}`;
}

export class RunDialog {
  constructor(
    private readonly root: HTMLDialogElement,
    private readonly workflow: Workflow,
    private readonly handlers: RunDialogHandlers,
  ) {
    this.root.addEventListener("click", (event) => {
      if (event.target === this.root) this.close();
    });
  }

  open(): void {
    this.render();
    if (!this.root.open) {
      if (typeof this.root.showModal === "function") this.root.showModal();
      else this.root.setAttribute("open", "");
    }
    const firstField = this.root.querySelector<HTMLElement>("input, textarea");
    (firstField ?? this.root.querySelector<HTMLElement>(".modal__footer button"))?.focus();
  }

  close(): void {
    if (this.root.open && typeof this.root.close === "function") this.root.close();
    else this.root.removeAttribute("open");
  }

  private render(): void {
    this.root.replaceChildren();
    const header = document.createElement("div");
    header.className = "modal__header";
    const title = document.createElement("h2");
    title.className = "modal__title";
    title.textContent = t("runDialog.title");
    const close = document.createElement("button");
    close.type = "button";
    close.className = "modal__close";
    close.textContent = "×";
    close.title = t("runDialog.close");
    close.setAttribute("aria-label", t("runDialog.close"));
    close.addEventListener("click", () => this.close());
    header.append(title, close);

    const body = document.createElement("div");
    body.className = "modal__body";
    const hint = document.createElement("p");
    hint.className = "modal__hint";
    hint.textContent = t("runDialog.hint");
    body.appendChild(hint);

    const invalidFields = new Set<string>();
    let runButton: HTMLButtonElement | null = null;
    const invalidate = (name: string, message: string) => {
      invalidFields.add(name);
      if (runButton) runButton.disabled = true;
      return message;
    };
    const validate = (name: string) => {
      invalidFields.delete(name);
      if (runButton) runButton.disabled = invalidFields.size > 0;
    };

    const names = Object.keys(this.workflow.variables);
    if (names.length === 0) {
      const empty = document.createElement("p");
      empty.className = "muted";
      empty.textContent = t("runDialog.noVariables");
      body.appendChild(empty);
    }
    for (const name of names) {
      const variable = this.workflow.variables[name];
      const existingOverride = this.handlers.getOverride(name);
      const card = document.createElement("section");
      card.className = "run-variable";
      card.dataset.variableName = name;

      const cardHeader = document.createElement("div");
      cardHeader.className = "run-variable__header";
      const variableName = document.createElement("code");
      variableName.textContent = `{{${name}}}`;
      cardHeader.appendChild(variableName);
      const status = document.createElement("span");
      status.className = "run-variable__status";
      const secret = document.createElement("span");
      secret.className = "badge badge--warn";
      secret.textContent = t("inspector.secret");
      secret.hidden = !variable.secret;
      const overrideBadge = document.createElement("span");
      overrideBadge.className = "badge badge--ok";
      overrideBadge.textContent = t("runDialog.overrideSet");
      overrideBadge.hidden = existingOverride === undefined;
      status.append(secret, overrideBadge);
      cardHeader.appendChild(status);
      card.appendChild(cardHeader);

      if (variable.description) {
        const description = document.createElement("p");
        description.className = "run-variable__description";
        description.textContent = variable.description;
        card.appendChild(description);
      }

      const defaultText = document.createElement("p");
      defaultText.className = "run-variable__default";
      defaultText.textContent = variable.secret
        ? t("runDialog.secretStored")
        : t("runDialog.defaultValue", { value: formatValue(variable.value) });
      card.appendChild(defaultText);

      const kind = variableKind(variable);
      const error = document.createElement("p");
      error.className = "field__hint field__hint--error";
      const markOverride = () => {
        overrideBadge.hidden = false;
        clear.disabled = false;
      };
      const commit = (value: unknown) => {
        this.handlers.setOverride(name, value);
        markOverride();
      };
      const clear = document.createElement("button");
      clear.type = "button";
      clear.className = "btn btn--small";
      clear.textContent = t("actions.clearOverride");
      clear.disabled = existingOverride === undefined;

      let resetControl: () => void = () => undefined;
      let control: HTMLInputElement | HTMLTextAreaElement;
      if (kind === "boolean") {
        const checkbox = document.createElement("input");
        checkbox.type = "checkbox";
        checkbox.checked = typeof existingOverride === "boolean"
          ? existingOverride
          : Boolean(variable.value);
        checkbox.addEventListener("change", () => commit(checkbox.checked));
        resetControl = () => {
          checkbox.checked = Boolean(variable.value);
        };
        control = checkbox;
      } else if (kind === "number") {
        const number = document.createElement("input");
        number.type = "number";
        number.step = "any";
        number.value = String(existingOverride ?? variable.value ?? "");
        number.addEventListener("input", () => {
          if (number.value.trim() === "") {
            this.handlers.clearOverride(name);
            overrideBadge.hidden = true;
            clear.disabled = true;
            error.textContent = "";
            validate(name);
            return;
          }
          const next = Number(number.value);
          if (!Number.isFinite(next)) {
            error.textContent = invalidate(name, t("runDialog.invalidNumber"));
            return;
          }
          error.textContent = "";
          validate(name);
          commit(next);
        });
        resetControl = () => {
          number.value = variable.value === undefined ? "" : String(variable.value);
        };
        control = number;
      } else if (kind === "json") {
        const textarea = document.createElement("textarea");
        textarea.className = "input input--code";
        textarea.rows = 4;
        textarea.spellcheck = false;
        textarea.value = JSON.stringify(existingOverride ?? variable.value, null, 2);
        textarea.addEventListener("input", () => {
          try {
            const next = JSON.parse(textarea.value);
            error.textContent = "";
            validate(name);
            commit(next);
          } catch (parseError) {
            error.textContent = invalidate(
              name,
              t("runDialog.invalidJson", { message: (parseError as Error).message }),
            );
          }
        });
        resetControl = () => {
          textarea.value = JSON.stringify(variable.value, null, 2);
        };
        control = textarea;
      } else {
        const text = document.createElement("input");
        text.type = variable.secret ? "password" : "text";
        text.value = variable.secret && existingOverride === undefined
          ? ""
          : String(existingOverride ?? variable.value ?? "");
        text.placeholder = variable.secret
          ? t("runDialog.enterSecret")
          : t("runDialog.overridePlaceholder");
        text.addEventListener("input", () => {
          if (variable.secret && text.value === "") return;
          commit(text.value);
        });
        resetControl = () => {
          text.value = variable.secret ? "" : String(variable.value ?? "");
        };
        control = text;
      }
      control.id = fieldId(name);
      control.dataset.variableName = name;
      control.setAttribute("aria-label", t("runDialog.overrideValue", { name }));
      control.className = "input";
      if (control instanceof HTMLTextAreaElement) control.classList.add("input--code");
      card.appendChild(control);
      card.appendChild(error);

      clear.addEventListener("click", () => {
        this.handlers.clearOverride(name);
        overrideBadge.hidden = true;
        clear.disabled = true;
        error.textContent = "";
        validate(name);
        resetControl();
      });
      cardHeader.appendChild(clear);
      body.appendChild(card);
    }

    const footer = document.createElement("div");
    footer.className = "modal__footer";
    const cancel = document.createElement("button");
    cancel.type = "button";
    cancel.className = "btn";
    cancel.textContent = t("actions.cancel");
    cancel.addEventListener("click", () => this.close());
    runButton = document.createElement("button");
    runButton.type = "button";
    runButton.className = "btn btn--primary";
    runButton.textContent = t("actions.run");
    runButton.disabled = invalidFields.size > 0;
    runButton.addEventListener("click", () => {
      if (runButton!.disabled) return;
      this.close();
      this.handlers.onRun();
    });
    footer.append(cancel, runButton);

    const form = document.createElement("div");
    form.className = "modal__form";
    form.addEventListener("keydown", (event) => {
      if (event.key === "Enter" && !(event.target instanceof HTMLTextAreaElement)) {
        event.preventDefault();
        runButton!.click();
      }
    });
    form.append(body, footer);
    this.root.append(header, form);
  }
}
