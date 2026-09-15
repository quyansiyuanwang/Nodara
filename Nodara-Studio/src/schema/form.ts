/**
 * A small JSON Schema renderer.
 *
 * It covers the subset node descriptors actually use: strings, numbers,
 * booleans, enums, nested objects and free-form values. Anything it cannot
 * render falls back to a JSON textarea instead of silently losing the value.
 */

import { t } from "../i18n";
import { JsonSchema } from "../runtime/types";

export interface FieldOptions {
  onChange: (value: unknown) => void;
  required?: boolean;
  idPrefix?: string;
}

function label(path: string, schema: JsonSchema): string {
  return schema.title ?? path;
}

function coerce(raw: string, schema: JsonSchema): unknown {
  const type = Array.isArray(schema.type) ? schema.type[0] : schema.type;
  switch (type) {
    case "number":
    case "integer": {
      if (raw.trim() === "") return undefined;
      const value = Number(raw);
      return Number.isFinite(value) ? value : raw;
    }
    case "boolean":
      return raw === "true";
    default:
      return raw;
  }
}

function examplePlaceholder(schema: JsonSchema): string {
  const example = schema.examples?.find((value) => typeof value === "string");
  if (typeof example === "string") return example;
  if (schema.format) return schema.format;
  return "";
}

function cloneValue<T>(value: T): T {
  return typeof structuredClone === "function"
    ? structuredClone(value)
    : JSON.parse(JSON.stringify(value)) as T;
}

function defaultArrayItem(schema: JsonSchema): unknown {
  if (schema.default !== undefined) return cloneValue(schema.default);
  const type = Array.isArray(schema.type) ? schema.type[0] : schema.type;
  switch (type) {
    case "number":
    case "integer":
      return 0;
    case "boolean":
      return false;
    case "array":
      return [];
    case "object":
      return {};
    default:
      return "";
  }
}

function arrayButton(action: string, label: string): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "array__button";
  button.dataset.action = action;
  button.title = label;
  button.setAttribute("aria-label", label);
  button.textContent =
    action === "add" || action === "remove"
      ? action === "add" ? "+" : "×"
      : action === "up" ? "↑" : "↓";
  return button;
}

/** Render one field, appending it to `parent`. */
export function renderField(
  parent: HTMLElement,
  key: string,
  schema: JsonSchema,
  value: unknown,
  options: FieldOptions,
): void {
  const wrapper = document.createElement("div");
  wrapper.className = "field";

  const id = `${options.idPrefix ?? "field"}-${key.replace(/[^A-Za-z0-9]/g, "-")}`;
  const fieldHeader = document.createElement("div");
  fieldHeader.className = "field__header";
  const headingGroup = document.createElement("div");
  headingGroup.className = "field__heading";
  const heading = document.createElement("label");
  heading.className = "field__label";
  heading.htmlFor = id;
  heading.textContent = `${label(key, schema)}${options.required ? " *" : ""}`;
  heading.dataset.required = options.required ? "true" : "false";
  if (schema.description) heading.title = schema.description;
  headingGroup.appendChild(heading);
  if (schema.description) {
    const info = document.createElement("span");
    info.className = "field__info";
    info.tabIndex = 0;
    info.setAttribute("role", "note");
    info.setAttribute("aria-label", schema.description);
    info.textContent = "?";
    const tooltip = document.createElement("span");
    tooltip.className = "field__tooltip";
    tooltip.textContent = schema.description;
    info.appendChild(tooltip);
    headingGroup.appendChild(info);
  }
  fieldHeader.appendChild(headingGroup);
  wrapper.appendChild(fieldHeader);

  const type = Array.isArray(schema.type) ? schema.type[0] : schema.type;
  let restoreDefault: (() => void) | null = null;

  if (type === "array") {
    wrapper.classList.add("field--array");
    let items: unknown[] = Array.isArray(value)
      ? value.slice()
      : Array.isArray(schema.default) ? cloneValue(schema.default) : [];
    const list = document.createElement("div");
    list.className = "array";
    const add = arrayButton("add", t("form.addItem"));
    add.addEventListener("click", () => {
      items.push(defaultArrayItem(schema.items ?? {}));
      options.onChange(items.slice());
      renderItems();
    });
    fieldHeader.appendChild(add);

    const renderItems = () => {
      list.replaceChildren();
      add.disabled = schema.maxItems !== undefined && items.length >= schema.maxItems;
      if (items.length === 0) {
        const empty = document.createElement("p");
        empty.className = "muted array__empty";
        empty.textContent = t("form.emptyArray");
        list.appendChild(empty);
        return;
      }
      items.forEach((item, index) => {
        const row = document.createElement("div");
        row.className = "array__item";
        row.dataset.index = String(index);
        const control = document.createElement("div");
        control.className = "array__control";
        renderField(control, String(index + 1), schema.items ?? {}, item, {
          idPrefix: `${id}-${index}`,
          onChange: (next) => {
            items[index] = next;
            options.onChange(items.slice());
          },
        });

        const actions = document.createElement("div");
        actions.className = "array__actions";
        const up = arrayButton("up", t("form.moveItemUp"));
        up.disabled = index === 0;
        up.addEventListener("click", () => {
          [items[index - 1], items[index]] = [items[index], items[index - 1]];
          options.onChange(items.slice());
          renderItems();
        });
        const down = arrayButton("down", t("form.moveItemDown"));
        down.disabled = index === items.length - 1;
        down.addEventListener("click", () => {
          [items[index + 1], items[index]] = [items[index], items[index + 1]];
          options.onChange(items.slice());
          renderItems();
        });
        const remove = arrayButton("remove", t("form.removeItem"));
        remove.disabled = schema.minItems !== undefined && items.length <= schema.minItems;
        remove.addEventListener("click", () => {
          items.splice(index, 1);
          options.onChange(items.slice());
          renderItems();
        });
        actions.append(up, down, remove);
        row.append(control, actions);
        list.appendChild(row);
      });
    };

    restoreDefault = () => {
      items = Array.isArray(schema.default) ? cloneValue(schema.default) : [];
      options.onChange(items.slice());
      renderItems();
    };
    renderItems();
    wrapper.appendChild(list);
  } else if (type === "object" && schema.properties && Object.keys(schema.properties).length > 0) {
    wrapper.classList.add("field--group");
    const objectValue =
      value && typeof value === "object" && !Array.isArray(value)
        ? (value as Record<string, unknown>)
        : {};
    for (const [childKey, childSchema] of Object.entries(schema.properties)) {
      renderField(wrapper, childKey, childSchema, objectValue[childKey], {
        required: schema.required?.includes(childKey),
        idPrefix: id,
        onChange: (childValue) => options.onChange({ ...objectValue, [childKey]: childValue }),
      });
    }
  } else if (schema.enum) {
    const select = document.createElement("select");
    select.id = id;
    select.className = "input";
    select.required = options.required === true;
    for (const option of schema.enum) {
      const element = document.createElement("option");
      element.value = String(option);
      element.textContent = String(option);
      select.appendChild(element);
    }
    select.value = value === undefined ? String(schema.default ?? schema.enum[0]) : String(value);
    select.addEventListener("change", () => options.onChange(coerce(select.value, schema)));
    restoreDefault = () => {
      const next = schema.default ?? schema.enum?.[0];
      select.value = String(next);
      options.onChange(next);
    };
    wrapper.appendChild(select);
  } else if (type === "boolean") {
    const checkbox = document.createElement("input");
    checkbox.id = id;
    checkbox.type = "checkbox";
    checkbox.required = options.required === true;
    checkbox.checked = Boolean(value ?? schema.default ?? false);
    checkbox.addEventListener("change", () => options.onChange(checkbox.checked));
    restoreDefault = () => {
      const next = Boolean(schema.default ?? false);
      checkbox.checked = next;
      options.onChange(next);
    };
    wrapper.appendChild(checkbox);
  } else if (type === "number" || type === "integer") {
    const input = document.createElement("input");
    input.id = id;
    input.className = "input";
    input.type = "number";
    input.required = options.required === true;
    if (schema.minimum !== undefined) input.min = String(schema.minimum);
    if (schema.maximum !== undefined) input.max = String(schema.maximum);
    if (schema.step !== undefined) input.step = String(schema.step);
    else if (type === "integer") input.step = "1";
    input.value = value === undefined || value === null ? "" : String(value);
    input.addEventListener("input", () => options.onChange(coerce(input.value, schema)));
    restoreDefault = () => {
      input.value = schema.default === undefined ? "" : String(schema.default);
      options.onChange(schema.default);
    };
    wrapper.appendChild(input);
  } else if (type === "string") {
    const long = (schema.description ?? "").length > 60 || key === "message";
    const input = document.createElement(long ? "textarea" : "input") as
      | HTMLTextAreaElement
      | HTMLInputElement;
    input.id = id;
    input.className = "input";
    input.required = options.required === true;
    if (!long) (input as HTMLInputElement).type = "text";
    if (long) {
      (input as HTMLTextAreaElement).rows = 3;
    } else {
      (input as HTMLInputElement).type = "text";
    }
    input.placeholder = examplePlaceholder(schema);
    if (schema.minLength !== undefined) input.minLength = schema.minLength;
    if (schema.maxLength !== undefined) input.maxLength = schema.maxLength;
    if (schema.pattern !== undefined && !long) {
      (input as HTMLInputElement).pattern = schema.pattern;
    }
    input.value = value === undefined || value === null ? "" : String(value);
    input.addEventListener("input", () => options.onChange(input.value));
    restoreDefault = () => {
      const next = schema.default === undefined ? "" : String(schema.default);
      input.value = next;
      options.onChange(schema.default);
    };
    wrapper.appendChild(input);
  } else {
    // Objects without a declared shape, arrays and anything unexpected: a JSON
    // editor that never loses the value.
    const textarea = document.createElement("textarea");
    textarea.id = id;
    textarea.className = "input input--code";
    textarea.rows = 4;
    textarea.spellcheck = false;
    textarea.required = options.required === true;
    textarea.value = JSON.stringify(value ?? schema.default ?? {}, null, 2);
    const errorHint = document.createElement("p");
    errorHint.className = "field__hint field__hint--error";
    textarea.addEventListener("input", () => {
      try {
        const parsed = JSON.parse(textarea.value);
        errorHint.textContent = "";
        options.onChange(parsed);
      } catch (error) {
        errorHint.textContent = t("form.invalidJson", { message: (error as Error).message });
      }
    });
    restoreDefault = () => {
      const next = schema.default ?? {};
      textarea.value = JSON.stringify(next, null, 2);
      errorHint.textContent = "";
      options.onChange(next);
    };
    wrapper.append(textarea, errorHint);
  }

  if (restoreDefault && schema.default !== undefined) {
    const reset = document.createElement("button");
    reset.type = "button";
    reset.className = "field__reset";
    reset.textContent = t("form.resetDefault");
    reset.addEventListener("click", restoreDefault);
    fieldHeader.appendChild(reset);
  }

  if (schema.description) {
    const hint = document.createElement("p");
    hint.className = "field__hint";
    hint.textContent = schema.description;
    wrapper.appendChild(hint);
  }

  parent.appendChild(wrapper);
}

/** Render a whole configuration object. */
export function renderConfigForm(
  parent: HTMLElement,
  schema: JsonSchema,
  config: Record<string, unknown>,
  onChange: (key: string, value: unknown) => void,
): void {
  const properties = schema?.properties ?? {};
  const keys = Object.keys(properties);
  if (keys.length === 0) {
    const note = document.createElement("p");
    note.className = "muted";
    note.textContent = t("form.noConfiguration");
    parent.appendChild(note);
  }
  for (const key of keys) {
    renderField(parent, key, properties[key], config[key], {
      required: schema.required?.includes(key),
      idPrefix: "field",
      onChange: (value) => onChange(key, value),
    });
  }
}
