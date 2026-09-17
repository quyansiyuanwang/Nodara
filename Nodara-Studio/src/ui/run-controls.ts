import { RunStatus } from "../runtime/types";

/** State of the automatic validation pass shown beside the toolbar. */
export type ValidationState = "unknown" | "checking" | "valid" | "invalid" | "unavailable";

/**
 * Why Run cannot start, so the toolbar can say so.
 *
 * A disabled button receives no clicks, and a missing plugin node type or an
 * unreachable runtime is only visible in the Problems tab or the connection
 * badge. Naming the reason in the tooltip is the feedback the click cannot give.
 */
export type RunBlockReason = "offline" | "localProblems" | "validation" | null;

export interface RunControls {
  runDisabled: boolean;
  /** Restart stops the active run and starts it again, so it needs one. */
  restartDisabled: boolean;
  pauseDisabled: boolean;
  resumeDisabled: boolean;
  stepDisabled: boolean;
  cancelDisabled: boolean;
  /** `null` while Run is available or a run is already in flight. */
  runBlockReason: RunBlockReason;
}

/**
 * Keep run controls independent from canvas rendering. In particular, "no
 * active run" (null) is runnable; only an actually in-flight run disables Run.
 */
export function deriveRunControls(
  status: RunStatus | null,
  connected: boolean,
  validation: ValidationState,
  localErrorCount: number,
  runStarting = false,
): RunControls {
  const inFlight =
    runStarting || status === "pending" || status === "running" || status === "paused";
  const paused = status === "paused";
  const terminal = status === "completed" || status === "failed" || status === "cancelled";
  const validationBlocked = validation === "invalid" || validation === "unavailable";
  const blocked = inFlight || !connected || localErrorCount > 0 || validationBlocked;
  const stepBlocked = !connected || localErrorCount > 0 || validationBlocked;

  const runBlockReason: RunBlockReason = inFlight
    ? null
    : !connected
      ? "offline"
      : localErrorCount > 0
        ? "localProblems"
        : validationBlocked
          ? "validation"
          : null;

  return {
    runDisabled: blocked,
    restartDisabled: !inFlight,
    pauseDisabled: !inFlight || paused || runStarting,
    resumeDisabled: !paused,
    stepDisabled: runStarting || stepBlocked || !(paused || status === null || terminal),
    cancelDisabled: !inFlight,
    runBlockReason,
  };
}
