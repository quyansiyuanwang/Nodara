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
