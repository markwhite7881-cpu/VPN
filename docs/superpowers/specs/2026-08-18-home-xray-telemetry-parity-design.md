# Home telemetry parity for Xray — design

**Date:** 2026-08-18
**Status:** approved for specification review
**Scope:** make the Home tab report consistent connection state, traffic, country flag, and latency for sing-box and Xray profiles; correct the connected-state power button.

## Goal

A user who connects through an Xray-backed subscription should see the same useful Home-tab information as with sing-box:

- current download and upload speed, plus per-session totals;
- a country flag and a latency badge beside the selected server/profile where that information can be established safely;
- a clearly green primary connection button while connected.

The implementation must not disclose subscription URLs, provider configuration, proxy credentials, server hostnames, ports, runtime-config locations, or Xray process arguments to the WebView.

## Non-goals

- No change to existing sing-box traffic collection, proxy switching, URL-test selection, profile selection, subscriptions, Routing 2.0, or system-proxy ownership.
- No change to the Xray engine selection or fallback policy.
- No Xray binary auto-updater in this feature. It requires a separate security design and validation plan.
- No raw endpoint metadata in frontend types, logs, errors, or Tauri commands.

## Current limitation

The frontend uses a single `traffic` event, but the event producer currently starts only for sing-box's Clash API WebSocket. Xray starts without a controller URL and therefore emits no samples; Home falls back to zero values.

Ready Xray subscription profiles intentionally expose only a safe summary to the frontend. Because they lack an endpoint, Home currently cannot derive a country or run its existing direct TCP latency probe; it deliberately renders the global fallback flag and no latency.

## Design

### 1. Engine-neutral traffic event

Keep the existing frontend contract unchanged:

```ts
type TrafficSample = {
  up_bps: number;
  down_bps: number;
  up_total: number;
  down_total: number;
  ts_ms: number;
};
```

`useTrafficStream` and the Home speed cards continue to consume the `traffic` Tauri event with no engine-specific branch.

Rust owns both traffic sources:

- **sing-box:** preserve the existing Clash `/traffic` WebSocket collection exactly.
- **Xray:** inject Xray's local stats configuration when preparing the runtime config, poll/read its local statistics endpoint from Rust only, calculate rates from cumulative counters, and emit the same `TrafficSample` event once per second.

The Xray stats listener must bind only to loopback, use a dynamically allocated local port, and never be reported to the WebView. It is started only for the active Xray run and is cancelled through the existing process lifecycle so a stale stream cannot survive an engine switch.

If Xray telemetry cannot be initialized, the connection itself remains available. Rust logs a sanitized internal warning; Home displays no fabricated values. Existing zero values remain an honest fallback when no sample has arrived.

### 2. Safe Xray Home metadata

Add a backend-owned safe presentation summary to the connection profile path. The WebView receives only:

```ts
type HomeProfileMetadata = {
  country_code: string | null; // ISO 3166-1 alpha-2, or null
  latency_ms: number | null;   // bounded TCP-connect result, or null
};
```

It must never receive a hostname, IP address, port, path, subscription URL, raw Xray JSON, UUID, transport detail, or authentication material.

For Xray ready profiles, Rust derives the candidate endpoint from the already-resolved trusted profile configuration, then:

1. resolves a country code using the same approved best-effort strategy used for the existing Home flag experience;
2. performs a bounded direct TCP-connect probe before or during Home metadata refresh;
3. exposes only the normalized country code and latency result through a new safe command/result field.

The frontend maps `country_code` to the existing `FlagIcon` and `codeToFlag` behavior. A null country code renders the existing global icon. A null latency renders the existing em dash/no-bars state; it must not be shown as `0 ms`.

Sing-box manual profiles retain the existing direct frontend probing path, avoiding regression. Subscription and ready-profile entries use backend-provided safe metadata where available.

### 3. Active connection presentation

`activeOutbound` is sing-box-specific and remains so. For Xray, Home uses the selected/started ready profile label and its safe metadata rather than pretending to know a selector member.

When running with Xray:

- the headline remains `Connected`;
- the subheading reads `via <flag> <safe profile label>`;
- no raw Xray engine or network diagnostics appear in the hero;
- the profile picker and server strip display its safe flag and latency when known.

### 4. Green connected power button

The primary power button gets distinct state tokens while preserving the existing design system:

- **Disconnected:** current neutral muted treatment.
- **Connecting / disconnecting:** current neutral spinner treatment.
- **Connected:** semantic green background, border, icon, hover, focus ring, and subtle shadow/glow based on existing semantic color tokens rather than hard-coded arbitrary palette colors.

The button still means “disconnect” while connected, retains its accessible label, and is disabled during transitions.

### 5. Updates card

The Updates card remains limited to:

1. **App** — Tauri application updater.
2. **VPN core** — existing sing-box runtime updater.

It must not list Xray as up-to-date, not detected, or updateable because no Xray updater exists today. This prevents a misleading security claim.

## Separate future feature: Xray updater

An Xray updater is feasible and valuable, but is intentionally a separate feature because it downloads and executes a security-sensitive binary. Its design must define:

- the official upstream release source and exact allowed asset names per platform/architecture;
- strict redirect, filename, archive, size, checksum/signature, and executable validation;
- storage under application data rather than beside bundled binaries;
- atomic staged installation, preservation of the last verified runtime, and rollback on failure;
- version detection and a UI row that only appears after the verified backend flow exists;
- no exposure of release URLs, local paths, archives, or validation internals to the WebView.

## Error handling and privacy

- Public UI errors are generic and actionable: `Traffic monitoring is temporarily unavailable`, `Latency unavailable`, or existing sanitized connection errors.
- Backend logs avoid configuration content and redact any sensitive network details.
- Failed traffic telemetry never stops a valid Xray connection.
- A failed metadata probe yields null metadata, not a fabricated flag or zero latency.

## Testing

### Rust

- Xray runtime preparation adds loopback-only stats configuration without changing provider routing or leaking an address into exposed types.
- Counter conversion returns correct rates/totals, handles the first sample, counter resets, malformed responses, and disconnects.
- Lifecycle tests prove a new Xray run owns one telemetry task and engine switches/stop/crash cancel it.
- Safe metadata serialization contains only `country_code` and `latency_ms`; regression tests assert host, port, URLs, credentials, paths, and raw configs cannot reach command results.
- Xray-ready profile metadata returns null safely when no eligible endpoint or probe result exists.

### Frontend

- Home renders traffic received through the existing event without checking engine kind.
- Xray presentation uses safe metadata for flag and latency, and falls back to the globe / unavailable state correctly.
- Connected button carries semantic green styling only in the running state and remains neutral during transitions.
- Updates card remains two rows until an Xray updater API is implemented.

### Regression gates

Run Rust formatting, library checks/tests, frontend tests, production frontend build, and `git diff --check`. Smoke-test both sing-box and Xray on Windows using a real connection, confirming traffic, safe server presentation, connect/disconnect behavior, and no system-proxy regression.

## Acceptance criteria

- An active Xray connection can emit non-zero Home download/upload values when traffic flows.
- A lack of Xray telemetry does not break the tunnel or invent readings.
- Xray ready profiles show a flag and latency only when safely resolved; otherwise they show the existing honest fallbacks.
- No sensitive configuration or endpoint detail crosses into the WebView.
- The running-state power button is visibly green and accessible.
- sing-box Home traffic, flags, latency, active selector display, and updater behavior remain unchanged.
- Updates does not misrepresent Xray update capability.
