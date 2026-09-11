/**
 * Content hints come from the published workflow schema.
 *
 * A document that declares `$schema` is completed by whatever JSON Schema
 * engine the editor uses; the Studio renders the same schema in its
 * configuration forms. These tests drive the engine VS Code itself uses
 * (`vscode-json-languageservice`) against the *published* schema, so a
 * regression in the generator fails here rather than silently costing every
 * editor its completion.
 */

import { describe, expect, it } from "vitest";
import {
  CompletionItem,
  getLanguageService,
  TextDocument,
} from "vscode-json-languageservice";

import schemaText from "../../../RecognizerFramework/schema/workflow.schema.json?raw";

/** A document inside `examples/`, where the relative `$schema` is valid. */
const DOCUMENT_URI = "file:///repo/examples/probe.json";
/** What `../RecognizerFramework/schema/workflow.schema.json` resolves to. */
const SCHEMA_URI = "file:///repo/RecognizerFramework/schema/workflow.schema.json";

const HEADER = `{
  "$schema": "../RecognizerFramework/schema/workflow.schema.json",
  "schema_version": "2.0",
  "id": "workflow.probe",
  "nodes": [
`;

function workflowText(body: string): string {
  return `${HEADER}${body}\n  ]\n}`;
}

function nodeText(node: string): string {
  return workflowText(`    ${node}`);
}

interface Harness {
  labels(text: string, marker: string, offset?: number): Promise<string[]>;
  documentation(text: string, marker: string, offset?: number): Promise<string[]>;
  diagnostics(text: string): Promise<string[]>;
}

function harness(requested: string[] = []): Harness {
  const service = getLanguageService({
    schemaRequestService: async (uri) => {
      requested.push(uri);
      return schemaText;
    },
    // Editors resolve a relative `$schema` against the document; the headless
    // service asks its host to do that through the workspace context.
    workspaceContext: {
      resolveRelativePath: (relativePath, resource) => new URL(relativePath, resource).toString(),
    },
  });
  service.configure({});

  const at = (text: string, marker: string, offset?: number) => {
    const position = text.indexOf(marker);
    expect(position, `marker ${marker} in the probe document`).toBeGreaterThanOrEqual(0);
    return text.indexOf(marker) + (offset ?? marker.length);
  };
  const documentationOf = (item: CompletionItem): string =>
    typeof item.documentation === "string"
      ? item.documentation
      : item.documentation?.value ?? "";

  return {
    async labels(text, marker, offset): Promise<string[]> {
      const document = TextDocument.create(DOCUMENT_URI, "json", 0, text);
      const completion = await service.doComplete(
        document,
        document.positionAt(at(text, marker, offset)),
        service.parseJSONDocument(document),
      );
      return (completion?.items ?? []).map((item) => item.label);
    },
    async documentation(text, marker, offset): Promise<string[]> {
      const document = TextDocument.create(DOCUMENT_URI, "json", 0, text);
      const completion = await service.doComplete(
        document,
        document.positionAt(at(text, marker, offset)),
        service.parseJSONDocument(document),
      );
      return (completion?.items ?? []).map(
        (item) => `${item.label}: ${documentationOf(item)}`,
      );
    },
    async diagnostics(text): Promise<string[]> {
      const document = TextDocument.create(DOCUMENT_URI, "json", 0, text);
      const parsed = service.parseJSONDocument(document);
      // `message` is typed as `string | MarkupContent`; at runtime it is text.
      return (await service.doValidation(document, parsed)).map((item) =>
        String(item.message),
      );
    },
  };
}

describe("published workflow schema", () => {
  it("is resolved from the document's relative $schema", async () => {
    const requested: string[] = [];
    const engine = harness(requested);
    await engine.labels(nodeText(`{ "id": "log", "type": "core.Log" }`), '"id"', 0);
    expect(requested).toContain(SCHEMA_URI);
  });

  it("completes node types with the descriptor documentation", async () => {
    const engine = harness();
    const text = nodeText(`{ "id": "n", "type": "" }`);
    const items = await engine.documentation(text, '"type": ""', '"type": "'.length);

    const labels = items.map((item) => item.split(":")[0]);
    expect(labels).toContain('"core.Log"');
    expect(labels).toContain('"windows.Input.Keyboard"');
    expect(labels).toContain('"vision.Ocr"');
    expect(items.find((item) => item.startsWith('"core.Log"'))).toContain(
      "Writes a message to the run log",
    );
  });

  it("completes the configuration of the node's own type", async () => {
    const engine = harness();

    const log = await engine.labels(
      nodeText(`{ "id": "log", "type": "core.Log", "config": {  } }`),
      '"config": {',
      11,
    );
    expect(log).toEqual(expect.arrayContaining(["message", "level"]));

    const keyboard = await engine.labels(
      nodeText(`{ "id": "key", "type": "windows.Input.Keyboard", "config": {  } }`),
      '"config": {',
      11,
    );
    expect(keyboard).toContain("keys");
    // A branch keyed on another node type must not leak its keys in.
    expect(keyboard).not.toContain("message");
  });

  it("documents configuration keys", async () => {
    const engine = harness();
    const items = await engine.documentation(
      nodeText(`{ "id": "log", "type": "core.Log", "config": {  } }`),
      '"config": {',
      11,
    );
    expect(items.find((item) => item.startsWith("message:"))).toContain("Message template");
  });

  it("completes enum values", async () => {
    const engine = harness();
    const labels = await engine.labels(
      nodeText(`{ "id": "log", "type": "core.Log", "config": { "level": "" } }`),
      '"level": ""',
      '"level": "'.length,
    );
    expect(labels).toEqual(['"debug"', '"info"', '"warn"', '"error"']);
  });

  it("validates configuration against the node's own schema", async () => {
    const engine = harness();
    const messages = await engine.diagnostics(
      workflowText(`    { "id": "log", "type": "core.Log", "config": { "level": "loud" } }`),
    );
    const joined = messages.join("\n");
    // The required key is reported…
    expect(joined).toContain('Missing property "message"');
    // …and the unknown enum value names the alternatives to suggest.
    expect(joined).toContain('Valid values: "debug", "info", "warn", "error"');
  });
});
