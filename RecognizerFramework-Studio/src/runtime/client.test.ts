import { afterEach, describe, expect, it, vi } from "vitest";

import { RuntimeClient, RuntimeError } from "./client";
import { NodeDescriptor } from "./types";

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("runtime client", () => {
  it("maps the node-type payload onto descriptors", async () => {
    const descriptor: Partial<NodeDescriptor> = { node_type: "core.Log" };
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse({ node_types: [descriptor] }));
    vi.stubGlobal("fetch", fetchMock);

    const client = new RuntimeClient();
    const types = await client.nodeTypes();
    expect(types).toHaveLength(1);
    expect(types[0].node_type).toBe("core.Log");
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/node-types",
      expect.objectContaining({ headers: { "content-type": "application/json" } }),
    );
  });

  it("surfaces the runtime's structured error", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        jsonResponse({ code: "E_RUN_NOT_FOUND", message: "no run with id `abc`" }, 404),
      ),
    );
    const client = new RuntimeClient();
    await expect(client.getRun("abc")).rejects.toMatchObject({
      name: "RuntimeError",
      code: "E_RUN_NOT_FOUND",
      status: 404,
    });
  });

  it("reports a validation failure body verbatim", async () => {
    const detail = { diagnostics: [{ code: "WF113" }] };
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        jsonResponse({ code: "E_WORKFLOW_INVALID", message: "invalid", detail }, 422),
      ),
    );
    const client = new RuntimeClient();
    try {
      await client.createRun({
        schema_version: "2.0",
        id: "wf",
        metadata: { name: "wf", tags: [] },
        nodes: [],
        edges: [],
        variables: {},
      });
      throw new Error("expected a rejection");
    } catch (error) {
      expect(error).toBeInstanceOf(RuntimeError);
      expect((error as RuntimeError).detail).toEqual(detail);
    }
  });

  /**
   * The Studio polls the runtime, so a drop must be survivable: the failure has
   * to surface, and the very next call must be able to succeed with no sticky
   * state left behind.
   */
  it("recovers after a dropped connection", async () => {
    const fetchMock = vi
      .fn()
      .mockRejectedValueOnce(new TypeError("Failed to fetch"))
      .mockResolvedValueOnce(jsonResponse({ status: "ok", node_types: 6, plugins: 0, runs: 0 }));
    vi.stubGlobal("fetch", fetchMock);

    const client = new RuntimeClient();
    await expect(client.health()).rejects.toBeInstanceOf(TypeError);

    const health = await client.health();
    expect(health.status).toBe("ok");
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("steers a run through the control endpoints", async () => {
    // A fresh Response per call: a body can only be read once.
    const fetchMock = vi
      .fn()
      .mockImplementation(() => Promise.resolve(jsonResponse({ id: "r1", status: "paused" })));
    vi.stubGlobal("fetch", fetchMock);

    const client = new RuntimeClient();
    await client.pause("r1");
    expect(fetchMock).toHaveBeenCalledWith("/api/v1/runs/r1/pause", expect.anything());

    await client.decideApproval("s1", "a1", "approved", "tester");
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/agent/sessions/s1/approvals/a1",
      expect.objectContaining({ method: "POST" }),
    );
  });

  it("builds the WebSocket URL for a run", () => {
    const sockets: string[] = [];
    class FakeSocket {
      constructor(url: string) {
        sockets.push(url);
      }
      addEventListener() {}
      close() {}
    }
    vi.stubGlobal("WebSocket", FakeSocket as unknown as typeof WebSocket);

    const client = new RuntimeClient();
    const close = client.streamRunEvents("r1", { onEvent: () => undefined });
    expect(sockets[0]).toContain("/api/v1/runs/r1/events");
    expect(sockets[0].startsWith("ws")).toBe(true);
    close();
  });
});
