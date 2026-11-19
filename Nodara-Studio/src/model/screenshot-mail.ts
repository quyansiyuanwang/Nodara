import example from "../../../examples/screenshot-mail.json?raw";
import { CATALOGS } from "../i18n";
import type { Workflow } from "../runtime/types";

export function screenshotMailWorkflow(): Workflow { return JSON.parse(example) as Workflow; }

export function isScreenshotMailGoal(goal: string): boolean {
  const normalize = (s: string) => s.trim().toLowerCase().replace(/[\s.,!?，。！？]+/g, "");
  return Object.values(CATALOGS).some(c => normalize(goal) === normalize(c["recipe.goal"]));
}
