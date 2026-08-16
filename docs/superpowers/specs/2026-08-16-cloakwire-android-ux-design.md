# Cloakwire Android UX refinement design

## Goal

Make the mobile client faster to operate daily without adding configuration complexity: make per-app routing legible, enable deliberate tab swipes, make pending connection changes explicit, simplify adding sources, and improve server and connection feedback.

## Scope

### Per-app routing

- In **Only selected** and **All except selected**, apps in `tun_app_list` are sorted before all other apps.
- The selected group retains the existing internet-capable then alphabetical order; remaining apps use the same order.
- Search applies the same selected-first ordering to matches.
- In **All apps**, use the existing normal order.
- The collapsed picker lists up to two selected app labels, then `+N`; with no apps it says `Select apps…`.

### Tab navigation

- A horizontal swipe in the central content area moves one tab left or right.
- Only commit when horizontal movement is at least 72 CSS pixels and greater than vertical movement.
- Do not switch at the first/last tab.
- Ignore gestures initiated by interactive controls (`button`, `input`, `textarea`, `select`, links), scrollable lists, or text selection, preserving normal vertical scrolling and app-list interactions.
- Bottom navigation remains available and authoritative for direct tab choice.

### Active connection summary

- On Home, present a compact pressable status row showing the chosen server (or Auto), global routing mode, and per-app policy summary.
- The row routes to Servers for a server detail press and Routing for a policy press, without adding a new settings surface.

### Pending reconnection

- While the VPN is running, changing a server selection or configuration that is consumed at VPN start sets a local `connectionDirty` flag.
- A compact persistent banner appears above bottom navigation: `Settings changed` and `Reconnect`.
- Reconnect performs the existing orderly `disconnect → short state commit delay → connect` sequence.
- The flag is cleared only after a successful state transition to running; it stays visible after an error.
- Changes made while stopped never show the banner.

### Empty states

- Servers: instruct to paste a subscription or share link.
- Routing: explain that selecting Only selected enables individual app tunnelling.
- Logs: state that log entries appear after a connection begins.
- Use existing card, foreground, muted foreground, and border tokens; no new palette.

### Server latency status

- Retain the numeric ping.
- Add a small semantic dot: green `<250ms`, amber `250–699ms`, red `≥700ms` or unavailable, muted when not yet measured.
- Do not alter selection, test sequencing, or profile storage.

### Connection check

- On Home while connected, expose `Check connection`.
- Use the existing app/core diagnostic capability where available; otherwise report that verification needs an active VPN and surface a precise failure message.
- The UI reports success with elapsed milliseconds or failure, without treating the Android VPN indicator as proof.

### Unified source input

- Replace separate source-type tabs with one field and one Add action.
- Classify the trimmed value before mutation:
  - Known share scheme (`vless`, `vmess`, `trojan`, `ss`, `hysteria`, `hy2`) → parse and add direct profile(s).
  - `http`/`https` → create a subscription and fetch it through the existing subscription path.
  - Any other value → inline validation message; do not save profiles or subscriptions.
- Existing imported subscriptions and manual profiles remain compatible.

## Architecture

- Keep source parsing/classification close to the current Servers-screen add UI, delegating actual direct-link parsing and subscription lifecycle to their existing hooks/utilities.
- Keep per-app sorting as a pure exported helper, independent of Android APIs, so it can be unit-tested.
- Keep gesture recognition in `MobileApp` because it owns tab state; it should only derive a neighboring `TabId` and never know screen internals.
- Hold the pending reconnect state at `MobileApp`, where both VPN state and persisted settings are present.

## Validation

- Unit tests cover selected-first ordering, collapsed-label summaries, source classification, ping band mapping, and horizontal-gesture thresholds/exclusions.
- Frontend production build must pass.
- Android APK must build with the existing ASCII toolchain workaround, install on device `3B15AV0166300000`, and be visually checked.
- Functional VPN validation remains evidence-based: test connection reports do not substitute for real `inbound/tun` and `outbound/vless` logs when validating routing changes.

## Out of scope

- Server list swipe actions (intentionally deferred to avoid conflict with global tab gestures).
- GeoIP, traffic charts, remote telemetry, or new routing semantics.
- Automatic reconnect without the user pressing Reconnect.
