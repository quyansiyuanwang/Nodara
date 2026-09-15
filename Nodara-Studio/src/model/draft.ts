import { Workflow } from "../runtime/types";

const DRAFT_KEY = "nodara.workflow.draft";

function storage(): Storage | null {
  try {
    return globalThis.sessionStorage ?? null;
  } catch {
    return null;
  }
}

/** Load the current tab's in-memory workflow draft, if it is structurally usable. */
export function loadDraft(): Workflow | null {
  const stored = storage()?.getItem(DRAFT_KEY);
  if (!stored) return null;
  try {
    const value = JSON.parse(stored) as Partial<Workflow>;
    if (!value || typeof value !== "object") return null;
    if (!Array.isArray(value.nodes) || !Array.isArray(value.edges)) return null;
    if (value.variables && typeof value.variables !== "object") return null;
    return value as Workflow;
  } catch {
    return null;
  }
}

/** Persist a draft for this tab only; closing the tab clears it. */
export function saveDraft(workflow: Workflow): void {
  try {
    storage()?.setItem(DRAFT_KEY, JSON.stringify(workflow));
  } catch {
    // Storage can be unavailable or full; editing must still work in memory.
  }
}
