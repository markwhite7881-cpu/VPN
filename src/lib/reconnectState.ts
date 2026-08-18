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
