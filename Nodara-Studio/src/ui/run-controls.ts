import { RunStatus } from "../runtime/types";

/** State of the automatic validation pass shown beside the toolbar. */
export type ValidationState = "unknown" | "checking" | "valid" | "invalid" | "unavailable";

export interface RunControls {
  runDisabled: boolean;
  pauseDisabled: boolean;
  resumeDisabled: boolean;
  stepDisabled: boolean;
  cancelDisabled: boolean;
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
  const blocked =
    inFlight ||
    !connected ||
    localErrorCount > 0 ||
    validation === "invalid" ||
    validation === "unavailable";

  return {
    runDisabled: blocked,
    pauseDisabled: !inFlight || paused || runStarting,
    resumeDisabled: !paused,
    stepDisabled: !paused,
    cancelDisabled: !inFlight,
  };
}
