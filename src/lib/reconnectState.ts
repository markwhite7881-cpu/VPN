import type { Status } from "./types";

/**
 * Preserve a pending settings-change flag once it has been raised while the
 * managed process was running. Stopped edits do not create the flag.
 */
export function nextReconnectRequired(
  reconnectRequired: boolean,
  isRunning: boolean,
): boolean {
  return reconnectRequired || isRunning;
}

/**
 * The reconnect notice remains available while reconnecting, after a running
 * settings edit, or when a failed reconnect leaves the process stopped.
 */
export function shouldShowReconnectNotice({
  reconnectInProgress,
  reconnectRequired,
  reconnectFailed,
  status,
}: {
  reconnectInProgress: boolean;
  reconnectRequired: boolean;
  reconnectFailed: boolean;
  status: Status;
}): boolean {
  return (
    reconnectInProgress ||
    (reconnectRequired && status === "running") ||
    (reconnectFailed && (status === "stopped" || status === "crashed"))
  );
}
