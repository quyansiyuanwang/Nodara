/**
 * A small JSON Schema renderer.
 *
 * It covers the subset a node descriptor actually uses — strings, numbers,
 * booleans, enums, objects and free-form values — which is exactly the subset
 * `rf-schema`'s descriptors are written in. Anything it does not understand
 * falls back to a JSON textarea rather than silently losing the value.
 */

import { JsonSchema } from "../runtime/types";

export interface FieldOptions {
  onChange: (value: unknown) => void;
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

  const id = `field-${key.replace(/[^A-Za-z0-9]/g, "-")}`;
  const heading = document.createElement("label");
  heading.className = "field__label";
  heading.htmlFor = id;
  heading.textContent = label(key, schema);
  if (schema.description) {
    heading.title = schema.description;
  }
  wrapper.appendChild(heading);

  const type = Array.isArray(schema.type) ? schema.type[0] : schema.type;

  if (schema.enum) {
    const select = document.createElement("select");
    select.id = id;
    select.className = "input";
    for (const option of schema.enum) {
      const element = document.createElement("option");
      element.value = String(option);
      element.textContent = String(option);
      select.appendChild(element);
    }
    select.value = value === undefined ? String(schema.default ?? schema.enum[0]) : String(value);
    select.addEventListener("change", () =>
      options.onChange(coerce(select.value, schema)),
    );
    wrapper.appendChild(select);
  } else if (type === "boolean") {
    const checkbox = document.createElement("input");
    checkbox.id = id;
    checkbox.type = "checkbox";
    checkbox.checked = Boolean(value ?? schema.default ?? false);
    checkbox.addEventListener("change", () => options.onChange(checkbox.checked));
    wrapper.appendChild(checkbox);
  } else if (type === "number" || type === "integer") {
    const input = document.createElement("input");
    input.id = id;
    input.className = "input";
    input.type = "number";
    if (schema.minimum !== undefined) input.min = String(schema.minimum);
    if (schema.maximum !== undefined) input.max = String(schema.maximum);
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
    if (!long) (input as HTMLInputElement).type = "text";
    if (long) (input as HTMLTextAreaElement).rows = 3;
    input.value = value === undefined || value === null ? "" : String(value);
    input.addEventListener("input", () => options.onChange(input.value));
    wrapper.appendChild(input);
  } else {
    // Objects, arrays and anything unexpected: a JSON editor that never loses
    // the value, with a syntax hint when it does not parse.
    const textarea = document.createElement("textarea");
    textarea.id = id;
    textarea.className = "input input--code";
    textarea.rows = 4;
    textarea.spellcheck = false;
    textarea.value = JSON.stringify(value ?? schema.default ?? {}, null, 2);
    const hint = document.createElement("p");
    hint.className = "field__hint";
    textarea.addEventListener("input", () => {
      try {
        const parsed = JSON.parse(textarea.value);
        hint.textContent = "";
        options.onChange(parsed);
      } catch (error) {
        hint.textContent = `invalid JSON: ${(error as Error).message}`;
      }
    });
    wrapper.appendChild(textarea);
    wrapper.appendChild(hint);
  }

  if (schema.description && type !== "string") {
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
    note.textContent = "This node has no configuration.";
    parent.appendChild(note);
  }
  for (const key of keys) {
    renderField(parent, key, properties[key], config[key], {
      onChange: (value) => onChange(key, value),
    });
  }
}
