# Automatic server reconnect and pending-settings notice

**Date:** 2026-08-18
**Branch:** `integration/v1.3.1-desktop`
**Status:** Approved design

## Goal

Make server selection immediately effective when the VPN is already running, while keeping other configuration changes explicit and predictable.

## User-visible behavior

### Server selection

- When the VPN is stopped, selecting a server only updates the selected profile.
- When the VPN is running, selecting a different server performs the existing reliable lifecycle:
  `stop -> regenerate configuration -> start`.
- The selected server remains the requested server after reconnect.
- A successful reconnect clears any pending-settings notice.
- Existing sing-box and Xray lifecycle boundaries remain authoritative; no raw provider data or engine internals are exposed to the WebView.

### Other settings

Changes made from Config or Routing are persisted immediately but do not interrupt a running VPN.

If the VPN is stopped, no notice is shown; the next manual connection uses the saved settings.

If the VPN is running, show a persistent, non-error notice:

> Settings saved. Reconnect the VPN to apply changes.

The notice includes:

> Reconnect now

The action uses the normal safe `stop -> regenerate -> start` flow. It disappears after a successful reconnect.

## Change classification

Do not infer the source of a change by comparing serialized settings. Server selection updates `default_outbound`, but that is a server-selection action, not a generic routing change.

Use explicit intent at the `App.tsx` boundary:

- `onSelectProfile` updates server-selection state and owns automatic reconnect behavior.
- Config and Routing receive a settings-change callback that marks `reconnectRequired` when the VPN is running.
- Reset-to-defaults is treated as a generic settings change.

## State model

Add UI state in `App.tsx` for whether the running process has unapplied settings. The state must:

- be false on initial load;
- become true only for generic settings changes while status is `running`;
- remain true while additional settings are edited;
- become false after a successful manual or automatic reconnect;
- not be shown when the VPN is stopped;
- be cleared when a reconnect attempt succeeds;
- retain the existing error state separately, so an unapplied-settings notice is not presented as a failure.

## Data flow

```text
Config/Routing change
  -> App settings handler
  -> persist settings
  -> if running: reconnectRequired = true
  -> show notice with Reconnect now

Server selection
  -> update selected profile/default_outbound
  -> if running: stop -> start
  -> clear reconnectRequired after success

Reconnect now
  -> clear system proxy as needed
  -> stop
  -> regenerate from current profiles + settings
  -> start
  -> reapply system proxy when applicable
  -> clear reconnectRequired after success
```

The implementation should reuse the existing `onStart`, `onStop`, and selected-profile handling rather than adding a second engine-specific start path.

## Error handling

- If automatic server reconnect fails, preserve the existing error banner behavior and do not claim the new server is active.
- If manual “Reconnect now” fails, keep the notice visible so the user can retry, and show the existing safe human-readable error.
- Do not expose raw Xray/sing-box errors, configuration contents, credentials, provider URLs, or runtime paths.
- Do not restart for tab switches, polling, subscription display-name updates, telemetry refresh, or initial settings hydration.

## UI placement

Place the notice in the shared App shell near the existing error/status presentation so it is visible regardless of whether the user is on Config or Routing. It should use the existing design-system surface, border, typography, and button components.

## Testing requirements

Add or update frontend tests for:

1. server selection while stopped does not call stop/start;
2. server selection while running uses the reconnect flow;
3. generic settings changes while stopped do not set a visible pending notice;
4. generic settings changes while running set the pending notice;
5. multiple generic changes keep one pending notice;
6. “Reconnect now” uses the current settings and clears the notice only after success;
7. failed reconnect preserves the notice and reports the existing safe error;
8. successful server reconnect clears a previously pending notice;
9. no restart is triggered by initial hydration or tab switching.

Run the frontend test suite, production build, and `git diff --check` before packaging a test installer.

## Scope boundaries

In scope:

- desktop App shell behavior;
- server-selection reconnect orchestration;
- pending-settings state and notice;
- frontend tests and unsigned integration build validation.

Out of scope:

- Android behavior or UI;
- automatic reconnect for every setting;
- Xray runtime redesign;
- updater/signing/release publication;
- exposing new backend metadata to the WebView.
