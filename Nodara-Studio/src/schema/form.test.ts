import { describe, expect, it } from "vitest";

import { renderConfigForm } from "./form";
import { JsonSchema } from "../runtime/types";

function render(schema: JsonSchema, config: Record<string, unknown>) {
  const host = document.createElement("div");
  const changes: Record<string, unknown> = {};
  renderConfigForm(host, schema, config, (key, value) => {
    changes[key] = value;
  });
  return { host, changes };
}

describe("schema-driven configuration forms", () => {
  it("renders one control per declared property", () => {
    const schema: JsonSchema = {
      type: "object",
      properties: {
        message: { type: "string" },
        retries: { type: "integer" },
        enabled: { type: "boolean" },
      },
    };
    const { host } = render(schema, {});
    expect(host.querySelectorAll(".field")).toHaveLength(3);
    expect(host.querySelector("textarea")).not.toBeNull();
    expect(host.querySelector('input[type="number"]')).not.toBeNull();
    expect(host.querySelector('input[type="checkbox"]')).not.toBeNull();
  });

  it("uses the enum as a select", () => {
    const schema: JsonSchema = {
      type: "object",
      properties: { level: { type: "string", enum: ["info", "warn", "error"] } },
    };
    const { host } = render(schema, { level: "warn" });
    const select = host.querySelector("select")!;
    expect(select.options).toHaveLength(3);
    expect(select.value).toBe("warn");
  });

  it("reports edits with the right coercion", () => {
    const schema: JsonSchema = {
      type: "object",
      properties: {
        message: { type: "string" },
        retries: { type: "integer" },
        enabled: { type: "boolean" },
      },
    };
    const { host, changes } = render(schema, {});

    const textarea = host.querySelector("textarea") as HTMLTextAreaElement;
    textarea.value = "hello";
    textarea.dispatchEvent(new Event("input"));
    expect(changes.message).toBe("hello");

    const number = host.querySelector('input[type="number"]') as HTMLInputElement;
    number.value = "7";
    number.dispatchEvent(new Event("input"));
    expect(changes.retries).toBe(7);

    const checkbox = host.querySelector('input[type="checkbox"]') as HTMLInputElement;
    checkbox.checked = true;
    checkbox.dispatchEvent(new Event("change"));
    expect(changes.enabled).toBe(true);
  });

  it("marks required fields and uses examples as placeholders", () => {
    const schema: JsonSchema = {
      type: "object",
      required: ["title"],
      properties: {
        title: { type: "string", title: "Title", examples: ["Notepad"] },
      },
    };
    const { host } = render(schema, {});
    expect(host.querySelector(".field__label")?.textContent).toBe("Title *");
    expect(host.querySelector<HTMLInputElement>("input")?.placeholder).toBe("Notepad");
  });

  it("resets a field to its schema default", () => {
    const schema: JsonSchema = {
      type: "object",
      properties: { retries: { type: "integer", default: 3 } },
    };
    const { host, changes } = render(schema, { retries: 9 });
    const input = host.querySelector<HTMLInputElement>('input[type="number"]')!;
    expect(input.value).toBe("9");
    host.querySelector<HTMLButtonElement>(".field__reset")!.click();
    expect(input.value).toBe("3");
    expect(changes.retries).toBe(3);
  });

  it("renders nested object fields without a JSON editor", () => {
    const schema: JsonSchema = {
      type: "object",
      properties: {
        window: {
          type: "object",
          title: "Window",
          required: ["title"],
          properties: {
            title: { type: "string", title: "Window title" },
          },
        },
      },
    };
    const { host, changes } = render(schema, { window: { title: "Notepad" } });
    const input = host.querySelector<HTMLInputElement>("#field-window-title")!;
    expect(input.value).toBe("Notepad");
    input.value = "Settings";
    input.dispatchEvent(new Event("input"));
    expect(changes.window).toEqual({ title: "Settings" });
  });

  it("falls back to a JSON editor for anything it cannot render", () => {
    const schema: JsonSchema = {
      type: "object",
      properties: { payload: { type: "object" } },
    };
    const { host, changes } = render(schema, { payload: { a: 1 } });
    const textarea = host.querySelector("textarea") as HTMLTextAreaElement;
    expect(JSON.parse(textarea.value)).toEqual({ a: 1 });

    textarea.value = "{ not json";
    textarea.dispatchEvent(new Event("input"));
    expect(host.querySelector(".field__hint")?.textContent).toContain("invalid JSON");

    textarea.value = '{"b":2}';
    textarea.dispatchEvent(new Event("input"));
    expect(changes.payload).toEqual({ b: 2 });
  });

  it("says so when a node has no configuration", () => {
    const { host } = render({ type: "object", properties: {} }, {});
    expect(host.textContent).toContain("no configuration");
  });
});
