import { beforeEach, describe, expect, it } from "vitest";

import { loadDraft, saveDraft } from "./draft";
import { starterWorkflow } from "./workflow";

describe("session workflow drafts", () => {
  beforeEach(() => {
    sessionStorage.clear();
  });

  it("round-trips a workflow through the current tab", () => {
    const workflow = starterWorkflow();
    workflow.metadata.name = "Draft";
    saveDraft(workflow);
    expect(loadDraft()).toEqual(workflow);
  });

  it("ignores malformed draft data", () => {
    sessionStorage.setItem("nodara.workflow.draft", '{"nodes":{}}');
    expect(loadDraft()).toBeNull();
  });
});
