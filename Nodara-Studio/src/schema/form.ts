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
  const heading = document.createElement("label");
  heading.className = "field__label";
  heading.htmlFor = id;
  heading.textContent = `${label(key, schema)}${options.required ? " *" : ""}`;
  heading.dataset.required = options.required ? "true" : "false";
  if (schema.description) heading.title = schema.description;
  wrapper.appendChild(heading);

  const type = Array.isArray(schema.type) ? schema.type[0] : schema.type;

  if (type === "object" && schema.properties && Object.keys(schema.properties).length > 0) {
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
    wrapper.appendChild(select);
  } else if (type === "boolean") {
    const checkbox = document.createElement("input");
    checkbox.id = id;
    checkbox.type = "checkbox";
    checkbox.required = options.required === true;
    checkbox.checked = Boolean(value ?? schema.default ?? false);
    checkbox.addEventListener("change", () => options.onChange(checkbox.checked));
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
    wrapper.append(textarea, errorHint);
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
