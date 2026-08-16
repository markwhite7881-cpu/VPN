# Cloakwire Android UX Refinement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve Cloakwire Android daily-use UX with selected-app prioritisation, guarded tab swipes, explicit reconnect state, unified source entry, richer status feedback, and refined empty/latency states.

**Architecture:** Keep pure UI decisions in small exported helpers so they can be deterministically tested without Tauri or Android. `MobileApp` continues to own tab state, persisted settings, profiles, and VPN lifecycle; child screens receive narrow callbacks. Reuse existing parsing (`api.parseInput`), subscription lifecycle (`useSubscriptions`), VPN lifecycle (`useVpnConnection`), theme tokens, and traffic primitives instead of adding dependencies.

**Tech Stack:** React 18, TypeScript 5, Vite 5, Tailwind CSS, lucide-react, Tauri 2 Android plugin bridge, existing Playwright installation for browser smoke checks.

## Global Constraints

- Do not add an npm dependency without explicit user approval; use existing React, TypeScript, Playwright, and project utilities.
- Preserve existing shadcn-style semantic tokens (`bg-card`, `text-foreground`, `text-muted-foreground`, `border-border`, `bg-accent`); do not introduce hard-coded palette values except the existing semantic latency tones.
- Android-only connection assertions require physical device `3B15AV0166300000`; never equate the Android VPN indicator with proven traffic forwarding.
- Preserve source-storage compatibility: direct profiles continue through `onAddLinks`, subscriptions through `useSubscriptions().add`.
- Do not auto-reconnect; show an explicit reconnect action only while a running connection has stale start-time configuration.
- Global tab swipes must not fire from controls, editable fields, scrollable containers, text selection, or primarily vertical movement.
- Build Android with existing ASCII Rustup/target/NDK environment workaround.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/mobile/lib/mobileUi.ts` | Pure helpers: app ordering/summary, source classification, latency band, and swipe eligibility/direction. |
| `src/mobile/screens/RoutingScreen.tsx` | Uses app-ordering helpers and exposes compact selected-app summary plus routing empty hint. |
| `src/mobile/components/AddSubscriptionSheet.tsx` | Single source input and pre-mutation classification; delegates parsing/subscription addition to existing callbacks. |
| `src/mobile/screens/ServersScreen.tsx` | Improved empty copy and semantic latency marker. |
| `src/mobile/screens/HomeScreen.tsx` | Compact connection summary, routing/server navigation callbacks, and actual connection-check feedback. |
| `src/mobile/screens/LogsScreen.tsx` | Keep existing empty state; only adjust copy if needed for the shared UX vocabulary. |
| `src/mobile/MobileApp.tsx` | Gesture ownership, dirty-connection state, reconnect banner, screen callback composition. |
| `src/lib/vpn.ts` | Optional thin bridge export only if Kotlin plugin exposes a real verification command; no fabricated success fallback. |
| `src-tauri/gen/android/app/src/main/java/ru/classquiz/singbox/vpn/VpnPlugin.kt` | Add a narrowly scoped connection-check command only after proving a real app/core check mechanism exists. |
| `src/mobile/lib/mobileUi.test.ts` or browser smoke script | Deterministic helper assertions using currently available test tooling. |

## Task 1: Establish Pure Mobile UX Helpers

**Files:**
- Create: `src/mobile/lib/mobileUi.ts`
- Create: `src/mobile/lib/mobileUi.test.ts` (only if a runnable existing test entry is found; otherwise create `scripts/check-mobile-ui.mjs`)
- Modify: `package.json` only if an already-installed runner is wired without adding dependencies

**Interfaces:**
- Produces `orderAppsForPicker(apps: AppEntry[], selectedPackages: ReadonlySet<string>, mode: "all" | "include" | "exclude"): AppEntry[]`.
- Produces `summarizeSelectedApps(apps: AppEntry[] | null, packageNames: string[]): string`.
- Produces `classifySourceInput(input: string): { kind: "empty" | "share" | "subscription" | "invalid"; value: string }`.
- Produces `latencyTone(ms: number | undefined): "pending" | "fast" | "medium" | "slow"`.
- Produces `swipeDirection({ dx, dy, startTarget }: SwipeInput): "previous" | "next" | null`.

- [ ] **Step 1: Inspect installed test entry points before adding test code**

Run:
```powershell
Get-ChildItem -Path . -Filter 'playwright.config.*' -File
Get-ChildItem -Path . -Recurse -Include '*.test.ts','*.test.tsx','*.spec.ts','*.spec.tsx' -File
Get-Content -Encoding UTF8 package.json
```

Expected: determine whether Playwright already has a configured executable test entry. Do not install Vitest/Jest.

- [ ] **Step 2: Write failing deterministic helper checks**

Assert all of the following exact behaviours:
```ts
const apps = [
  { label: "Chrome", packageName: "com.android.chrome", hasInternet: true, system: false },
  { label: "Telegram", packageName: "org.telegram.messenger", hasInternet: true, system: false },
  { label: "Clock", packageName: "com.android.deskclock", hasInternet: false, system: true },
];

expect(orderAppsForPicker(apps, new Set(["org.telegram.messenger"]), "include")
  .map((app) => app.packageName))
  .toEqual(["org.telegram.messenger", "com.android.chrome", "com.android.deskclock"]);
expect(summarizeSelectedApps(apps, ["org.telegram.messenger", "com.android.chrome", "missing.app"]))
  .toBe("Telegram, Chrome +1");
expect(classifySourceInput("vless://example").kind).toBe("share");
expect(classifySourceInput("https://provider.example/sub").kind).toBe("subscription");
expect(classifySourceInput("provider.example").kind).toBe("invalid");
expect(latencyTone(249)).toBe("fast");
expect(latencyTone(250)).toBe("medium");
expect(latencyTone(700)).toBe("slow");
expect(swipeDirection({ dx: -80, dy: 20, startTarget: null })).toBe("next");
expect(swipeDirection({ dx: -60, dy: 5, startTarget: null })).toBeNull();
```

- [ ] **Step 3: Run the check and verify a meaningful red failure**

Run the discovered existing test command, or the standalone check script through Node. Expected: fail because `mobileUi` does not yet export the helpers.

- [ ] **Step 4: Implement minimal pure helpers**

Implement stable sorting with selected-first only for include/exclude, sorting both selected and unselected groups by `hasInternet` descending then `label.localeCompare`. Do not remove unmatched selected package IDs from persisted state; summary labels unknown IDs as a count. Recognise schemes case-insensitively:
```ts
const SHARE_SCHEMES = new Set(["vless", "vmess", "trojan", "ss", "hysteria", "hy2"]);
```

For swipes, return a direction only if `Math.abs(dx) >= 72`, `Math.abs(dx) > Math.abs(dy)`, and `startTarget` is not within a control/editable/scrollable ancestor.

- [ ] **Step 5: Run the helper checks to green**

Expected: all behaviour assertions pass, then run `npm run build` to type-check helpers inside production compilation.

- [ ] **Step 6: Commit the isolated helper layer**

```powershell
git add src/mobile/lib/mobileUi.ts src/mobile/lib/mobileUi.test.ts scripts/check-mobile-ui.mjs package.json
git commit -m "feat(mobile): add UX policy helpers"
```
Only stage files that actually exist.

## Task 2: Prioritise and Summarise Per-App Routing Selections

**Files:**
- Modify: `src/mobile/screens/RoutingScreen.tsx:118-292`
- Modify: `src/mobile/lib/mobileUi.ts`
- Test: helper test/script from Task 1

**Interfaces:**
- Consumes `orderAppsForPicker` and `summarizeSelectedApps` from `mobileUi.ts`.
- Preserves `onListChange(l: string[])` and `tun_app_list` format.
- Produces selected-first visual ordering and collapsed labels without mutating persisted package order.

- [ ] **Step 1: Add a failing UI-level ordering scenario**

Extend helper assertions to prove that all mode returns normal ordering while include/exclude return selected-first. Add a summary scenario with zero, one, two, and three selected apps.

- [ ] **Step 2: Verify red**

Run the helper test/script. Expected: fail until the helper covers the precise mode/summary cases.

- [ ] **Step 3: Implement RoutingScreen integration**

Replace the current one-time sorted array and simple filter with:
```ts
const visibleApps = useMemo(
  () => orderAppsForPicker(filteredMatches, selected, mode),
  [filteredMatches, mode, selected],
);
const pickerLabel = summarizeSelectedApps(apps, appList);
```

Use `pickerLabel` in the collapsed button. When `mode === "all"`, do not show a stale selection picker. Add a small empty guidance card only when route rules and per-app selection are absent; it must not hide functional controls.

- [ ] **Step 4: Verify green**

Run helper checks and `npm run build`. Manually inspect browser preview: select Telegram, close and reopen picker, search, switch include/exclude/all, and confirm the original package IDs remain intact.

- [ ] **Step 5: Commit**

```powershell
git add src/mobile/screens/RoutingScreen.tsx src/mobile/lib/mobileUi.ts src/mobile/lib/mobileUi.test.ts scripts/check-mobile-ui.mjs
git commit -m "feat(mobile): prioritise selected routing apps"
```

## Task 3: Replace Dual Source Modes with Unified Input

**Files:**
- Modify: `src/mobile/components/AddSubscriptionSheet.tsx:1-206`
- Modify: `src/mobile/lib/mobileUi.ts`
- Modify: `src/mobile/screens/ServersScreen.tsx:107-112`
- Test: helper test/script from Task 1

**Interfaces:**
- Consumes `classifySourceInput(input)`.
- Preserves `onAdd({ name?: string; url: string })` and `onAddLinks(outbounds: Outbound[])`.
- Produces one textarea/input accepting a direct share link, multiple newline-separated share links, or `http(s)` subscription URL(s).

- [ ] **Step 1: Write failing classification checks for mixed input**

Add exact cases:
```ts
expect(classifySourceInput("  ").kind).toBe("empty");
expect(classifySourceInput("HY2://example").kind).toBe("share");
expect(classifySourceInput("http://provider.example").kind).toBe("subscription");
expect(classifySourceInput("ftp://provider.example").kind).toBe("invalid");
```
For multiline input, classify by passing full text to existing `api.parseInput` after checking that its first nonempty line is a recognised source class.

- [ ] **Step 2: Verify red**

Run helper checks. Expected: failure for missing multiline/empty semantics.

- [ ] **Step 3: Implement one-field sheet**

Remove `Mode`, mode buttons, dedicated name field, and separate URL/link state. Use one `source` textarea with placeholder:
```text
Paste a subscription URL or share link
https://provider.example/sub
vless://…
```

On submit:
1. trim and reject empty input;
2. use `api.parseInput(source)` once;
3. add parsed outbounds through `onAddLinks`;
4. add every returned subscription through `onAdd({ url })`;
5. if both arrays are empty, show the first parser failure or `Unsupported link or subscription URL.`;
6. close only if at least one profile/subscription was accepted.

Do not independently regex-validate away formats that the existing Rust parser supports.

- [ ] **Step 4: Update empty state copy**

Change Servers empty hint to tell the user to use `+` to paste either a subscription URL or direct share link.

- [ ] **Step 5: Verify green**

Run helper checks and `npm run build`. In preview, test a direct link, a valid HTTP subscription URL, malformed text, and a mixed multiline payload. Verify malformed content leaves all existing profiles/subscriptions unchanged.

- [ ] **Step 6: Commit**

```powershell
git add src/mobile/components/AddSubscriptionSheet.tsx src/mobile/screens/ServersScreen.tsx src/mobile/lib/mobileUi.ts src/mobile/lib/mobileUi.test.ts scripts/check-mobile-ui.mjs
git commit -m "feat(mobile): unify server source input"
```

## Task 4: Add Protected Tab Swipes and Pending-Reconnect State

**Files:**
- Modify: `src/mobile/MobileApp.tsx:1-257`
- Modify: `src/mobile/lib/mobileUi.ts`
- Test: helper test/script from Task 1

**Interfaces:**
- Consumes `swipeDirection`.
- Adds `connectionDirty: boolean`, `markConnectionDirty()`, and `reconnect(): Promise<void>` at `MobileApp` scope.
- Passes `onSettingsChange` wrapper and `onConnectionInputChanged` callbacks to screens that alter start-time data.

- [ ] **Step 1: Write failing swipe eligibility and direction checks**

Add cases for 72px threshold, predominantly vertical motion, a button target, an input target, and edge tab clamping. Expected helper API returns null for all unsafe motions.

- [ ] **Step 2: Verify red**

Run checks. Expected: fail before complete target/threshold handling exists.

- [ ] **Step 3: Implement gesture handling in MobileApp**

Add `touchstart`/`touchend` handlers to `<main>`, recording start X/Y and the original target. On end, call `swipeDirection`; map `previous`/`next` to neighboring indexes in `TABS`, clamped at edges. Do not call `preventDefault`, preserving native scroll.

- [ ] **Step 4: Implement dirty flag and explicit reconnect**

Create a `setMobileSettings(next)` wrapper:
```ts
const setMobileSettings = (next: GeneratorSettings) => {
  setSettings(next);
  if (vpn.state === "running") setConnectionDirty(true);
};
```

Wrap `onSelectProfile`, profile/subscription add/remove callbacks, and settings/routing callbacks so a running tunnel is marked dirty after start-time inputs change. Implement:
```ts
const reconnect = async () => {
  await vpn.disconnect();
  await new Promise((resolve) => window.setTimeout(resolve, 50));
  await vpn.connect();
};
```

Display `Settings changed` + `Reconnect` only when `connectionDirty && vpn.state === "running"`; disable button while `vpn.busy`. Clear dirty only in an effect that observes `vpn.state === "running"` after a reconnect attempt, not immediately on click.

- [ ] **Step 5: Verify green**

Run helper checks and `npm run build`. In preview: swipe home→servers and servers→home; test vertical scroll and a click/drag inside app list; change routing while running; verify the banner appears; reconnect and confirm it clears after running state.

- [ ] **Step 6: Commit**

```powershell
git add src/mobile/MobileApp.tsx src/mobile/lib/mobileUi.ts src/mobile/lib/mobileUi.test.ts scripts/check-mobile-ui.mjs
git commit -m "feat(mobile): add guarded tab swipes and reconnect prompt"
```

## Task 5: Improve Home Summary, Empty States, and Latency Signals

**Files:**
- Modify: `src/mobile/screens/HomeScreen.tsx:1-205`
- Modify: `src/mobile/screens/ServersScreen.tsx:226-255`
- Modify: `src/mobile/screens/LogsScreen.tsx:76-81` only if copy needs alignment
- Modify: `src/mobile/lib/mobileUi.ts`
- Test: helper test/script from Task 1

**Interfaces:**
- Consumes `latencyTone` and a routing-policy summary helper.
- Extends `HomeScreen` props with `settings: GeneratorSettings`, `onOpenRouting(): void`, and optional `onCheckConnection(): Promise<ConnectionCheckResult>` only after Task 6 defines that API.
- Produces press targets that route to Servers/Routing instead of new screens.

- [ ] **Step 1: Write failing latency boundary checks**

Add exact assertions that `undefined → pending`, `249 → fast`, `250 → medium`, `699 → medium`, `700 → slow`.

- [ ] **Step 2: Verify red**

Run checks. Expected: fail before thresholds use the specified `<250`, `250–699`, `≥700` policy.

- [ ] **Step 3: Implement latency dot and copy refinements**

Update `LatencyBadge` to render a tiny labelled/accessible dot next to the existing number. Use the helper tone and semantic token classes. Keep `—` for pending; show a red failure tone only when a ping was attempted and returned no value, not simply before measurement.

- [ ] **Step 4: Implement Home routing summary**

Below server selection, add a compact split pressable row:
```text
<server label or Auto>     <Global / Auto / Direct; all apps / only selected / all except selected>
```

Server region calls `onOpenServers`; policy region calls `onOpenRouting`. Use `settings.routing.tun_app_mode` and `tun_app_list` to render `Telegram only`, `N apps selected`, or `All apps` without fetching Android app labels in Home.

- [ ] **Step 5: Verify green**

Run helper checks and `npm run build`. Browser smoke-check server and routing summary presses, no-profile state, and each latency threshold.

- [ ] **Step 6: Commit**

```powershell
git add src/mobile/screens/HomeScreen.tsx src/mobile/screens/ServersScreen.tsx src/mobile/screens/LogsScreen.tsx src/mobile/lib/mobileUi.ts src/mobile/lib/mobileUi.test.ts scripts/check-mobile-ui.mjs
git commit -m "feat(mobile): clarify connection and latency status"
```

## Task 6: Implement a Real Connection Check Only If the Bridge Supports It

**Files:**
- Inspect first: `src-tauri/gen/android/app/src/main/java/ru/classquiz/singbox/vpn/VpnPlugin.kt`, `CloakwireVpnService.kt`, `src/lib/api.ts`, `src-tauri/src/commands.rs`
- Modify only if evidence supports a real mechanism: `VpnPlugin.kt`, `src/lib/vpn.ts`, `src/mobile/useVpnConnection.ts`, `src/mobile/screens/HomeScreen.tsx`
- Test: existing helper check plus Android device evidence

**Interfaces:**
- If supported, define `ConnectionCheckResult = { ok: boolean; latencyMs?: number; message?: string }`.
- Export `vpnCheckConnection(): Promise<ConnectionCheckResult>` only when the plugin implementation makes an HTTPS request that is actually routed by the active VPN.

- [ ] **Step 1: Trace available diagnostic capabilities before adding UI code**

Read the four inspection files in full. Identify whether one of these exists: a core URL test endpoint reachable through the active tunnel, a Kotlin protected/non-protected HTTP client with proof of routing, or an existing Rust command that queries Clash through the active controller.

- [ ] **Step 2: Decide the evidence-based route**

- If an existing core/controller request can verify traffic through the tunnel, write a failing bridge/API test or source-level check documenting expected JSON shape.
- If no mechanism can prove tunnel traversal, do **not** add a misleading check button. Record this outcome in the implementation report and leave the Home UI unchanged.

- [ ] **Step 3: Implement only the verified mechanism**

If Clash/controller `urltest` is the available proven path, invoke it and return elapsed milliseconds. If raw Android HTTPS is used, prove it is not protected/bypassing the VPN and that it emits corresponding core traffic; otherwise reject it.

- [ ] **Step 4: Add Home feedback**

Show `Check connection` only while the VPN is running and only when the bridge API exists. Use an in-place status line: checking spinner, green `Verified · N ms`, or precise destructive failure. Do not conflate service state, DNS, or the Android VPN indicator with success.

- [ ] **Step 5: Verify on physical Android device**

Build/install, start VPN, click check, then inspect `files/singbox/box.log` through:
```powershell
& $adb -s '3B15AV0166300000' shell run-as ru.classquiz.singbox sh -c 'tail -n 200 files/singbox/box.log'
```
Expected: real matching `inbound/tun` and/or known core outbound evidence, depending on the verified implementation. If logs do not prove routing, report the limitation and remove/disable the claim.

- [ ] **Step 6: Commit**

```powershell
git add src-tauri/gen/android/app/src/main/java/ru/classquiz/singbox/vpn/VpnPlugin.kt src/lib/vpn.ts src/mobile/useVpnConnection.ts src/mobile/screens/HomeScreen.tsx
git commit -m "feat(android): add evidence-based connection check"
```
Stage only files changed by the validated route.

## Task 7: End-to-End Validation and Android Delivery

**Files:**
- Modify only as needed for fixes found during verification.
- Artifact: `src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk`

**Interfaces:**
- Validates all prior interfaces together without changing persistence formats or core routing configuration.

- [ ] **Step 1: Run frontend verification**

```powershell
npm run build
```
Expected: TypeScript and Vite complete successfully. Record existing large-chunk warnings separately; do not treat them as this feature’s failure.

- [ ] **Step 2: Run deterministic UI helper checks**

Run the exact test/script established in Task 1. Expected: all sort, summary, classification, latency, and swipe cases pass.

- [ ] **Step 3: Build Android with established environment**

```powershell
$env:ANDROID_NDK_HOME = 'C:\Users\Public\cwdev\ndk'
$env:NDK_HOME = $env:ANDROID_NDK_HOME
$env:RUSTUP_HOME = 'C:\Users\Public\cwdev\rustup-home'
$env:CARGO_HOME = 'C:\Users\Алексей\.cargo'
$env:CARGO_TARGET_DIR = 'C:\Users\Public\cwdev\target'
$env:Path = 'C:\Users\Алексей\.cargo\bin;' + $env:Path
$ErrorActionPreference = 'Continue'
$PSNativeCommandUseErrorActionPreference = $false
npx tauri android build --apk --debug --target aarch64
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
```
Expected: universal debug APK produced.

- [ ] **Step 4: Install and inspect on device**

```powershell
$adb = 'C:\Users\Алексей\AppData\Local\Android\Sdk\platform-tools\adb.exe'
$apk = 'C:\Users\Алексей\.minimax-agent\projects\singbox-client\src-tauri\gen\android\app\build\outputs\apk\universal\debug\app-universal-debug.apk'
& $adb -s '3B15AV0166300000' install -r $apk
& $adb -s '3B15AV0166300000' shell monkey -p ru.classquiz.singbox 1
```
Verify: selected apps are top-ranked; collapsed summary names apps; one-field add sheet classifies inputs; swipe does not fire inside controls; reconnect banner only appears for a running stale config; Home row routes correctly; latency badge is readable.

- [ ] **Step 5: Validate per-app routing remains real**

Select only Telegram, reconnect manually, produce Telegram traffic, and inspect active config/logs. Require `include_package:["org.telegram.messenger"]` plus actual `inbound/tun[tun-in]` and matching `outbound/vless[...]` evidence. This UI package must not regress Build29 routing.

- [ ] **Step 6: Commit final verification fixes and report**

```powershell
git status --short
git add <only-feature-files>
git commit -m "feat(mobile): refine Android VPN UX"
```
Report separately: build outcome, APK installation, each visual flow checked, the connection-check decision/evidence, and any pre-existing warnings not fixed in scope.
