# PROXIES availability for Xray — design

**Date:** 2026-08-18

## Goal

Prevent the PROXIES card from presenting sing-box-only proxy controls as functional while Xray is the active engine.

## Chosen UX

When the connection status is `running` and its engine is `xray`:

- Keep the PROXIES card visible in its existing position.
- Replace the normal `live` state with an unavailable state.
- Do not call the Clash API (`list_proxies`, `select_proxy`, or `test_delay`).
- Show a concise warning that proxy-group switching and Clash latency tests are available only for sing-box.
- Tell the user that an Xray server is changed by selecting another ready profile on Home.
- Do not expose Xray API details, runtime ports, provider configuration, or raw errors.

For a stopped connection, preserve the existing `not running` experience. For a running sing-box connection, preserve the existing selector, URLTest, selection, and latency behavior unchanged.

## Architecture

`ProxiesCard` already receives `Status`, which includes the optional engine. Derive one local boolean:

```ts
const isXrayRunning = isRunning && status.engine === "xray";
```

Use it as the capability boundary:

1. `refresh` returns early and clears stale proxy data when `isXrayRunning` is true.
2. The polling effect does not start a three-second refresh timer for Xray.
3. The card header renders an unavailable badge rather than `live`.
4. The content renders the explanatory blocked state instead of an empty-groups or backend-error state.

No backend changes are necessary: the existing backend rejection remains defense in depth for callers outside this component.

## Error handling

A blocked Xray state is not an error. It must not show a red error banner or make an unavailable capability look like an Xray connection failure.

## Tests

Add a focused component test that renders `ProxiesCard` with a running Xray status and verifies:

- the warning is visible;
- the sing-box-only explanation is visible;
- `api.listProxies` is not invoked;
- no live proxy-group content is rendered.

Keep the current test coverage for Home and subscriptions unchanged.

## Out of scope

- Building a universal proxy-group controller for Xray.
- Exposing Xray HandlerService or provider configuration to the WebView.
- Changing subscription/profile selection behavior.
- Adding any controls that claim Xray supports Clash selectors or URLTest groups.
