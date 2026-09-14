import { describe, expect, it } from "vitest";

import { WorkflowHistory } from "./history";

describe("workflow history", () => {
  it("undoes and redoes snapshots", () => {
    const history = new WorkflowHistory("a");
    history.push("b");
    history.push("c");
    expect(history.canUndo()).toBe(true);
    expect(history.undo()).toBe("b");
    expect(history.undo()).toBe("a");
    expect(history.undo()).toBeNull();
    expect(history.redo()).toBe("b");
    expect(history.redo()).toBe("c");
    expect(history.redo()).toBeNull();
  });

  it("drops redo entries after a new edit and enforces its limit", () => {
    const history = new WorkflowHistory("a", 2);
    history.push("b");
    expect(history.undo()).toBe("a");
    history.push("c");
    expect(history.canRedo()).toBe(false);
    history.push("d");
    expect(history.undo()).toBe("c");
    expect(history.undo()).toBeNull();
  });
});
