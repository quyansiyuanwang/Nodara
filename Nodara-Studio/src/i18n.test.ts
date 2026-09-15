import { describe, expect, it } from "vitest";

import { localizeCategory, localizeDescriptor, setLocale, t } from "./i18n";
import { NodeDescriptor } from "./runtime/types";

function descriptor(): NodeDescriptor {
  return {
    node_type: "windows.Input.Keyboard",
    display_name: "Keyboard",
    category: "Input",
    description: "Sends a key or key chord to the focused window",
    version: "2.0.0",
    inputs: [],
    outputs: [],
    config_schema: {
      type: "object",
      properties: {
        keys: {
          type: "string",
          title: "Key chord",
          description: "Key or chord to send to the focused window.",
        },
      },
    },
    capabilities: [],
    permissions: [],
    dangerous: false,
    allows_additional_config: false,
  };
}

describe("Studio i18n", () => {
  it("switches UI messages between English and Chinese", () => {
    setLocale("en");
    expect(t("actions.run")).toBe("Run");
    setLocale("zh-CN");
    expect(t("actions.run")).toBe("运行");
    setLocale("en");
  });

  it("localizes node identity and configuration labels", () => {
    setLocale("zh-CN");
    const localized = localizeDescriptor(descriptor());
    expect(localized.display_name).toBe("键盘");
    expect(localized.category).toBe("Input");
    expect(localizeCategory(localized.category)).toBe("输入");
    expect(localized.config_schema.properties?.keys.title).toBe("按键/组合键");
    setLocale("en");
  });

  it("localizes common command input and output ports", () => {
    setLocale("zh-CN");
    const command = {
      ...descriptor(),
      node_type: "system.Command",
      display_name: "Command",
      inputs: [
        {
          name: "in",
          display_name: "In",
          kind: "input" as const,
          value_type: "any" as const,
          required: false,
        },
      ],
      outputs: [
        {
          name: "out",
          display_name: "Stdout",
          kind: "output" as const,
          value_type: "string" as const,
          required: false,
        },
        {
          name: "exit_code",
          display_name: "Exit code",
          kind: "output" as const,
          value_type: "number" as const,
          required: false,
        },
      ],
    };
    const localized = localizeDescriptor(command);
    expect(localized.display_name).toBe("命令");
    expect(localized.inputs[0].display_name).toBe("输入");
    expect(localized.outputs[0].display_name).toBe("标准输出");
    expect(localized.outputs[1].display_name).toBe("退出码");
    setLocale("en");
  });
});
