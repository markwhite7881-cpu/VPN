# Cloakwire Mobile Navigation Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move Logs into Settings and add smooth, direction-aware mobile tab transitions.

**Architecture:** `MobileApp` retains one active view state; Logs becomes a Settings-owned subview rather than a bottom navigation item. A small transition-direction state drives CSS slide-and-fade classes while existing `swipeDirection` remains the sole gesture eligibility gate.

**Tech Stack:** React 18, TypeScript, Tailwind CSS, lucide-react, existing Node deterministic checker.

## Global Constraints

- Do not add npm dependencies.
- Keep bottom navigation to Home, Servers, Routing, Settings.
- Preserve guarded swipe exclusions for controls, editable fields, scrollable containers, text selection, and vertical gestures.
- Use semantic shadcn tokens; preserve existing latency tones only.
- Respect `prefers-reduced-motion`.
- Do not change VPN configuration, reconnect handling, routing, logs bridge behavior, or persistence formats.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/mobile/MobileApp.tsx` | Four persistent tabs, Logs subview routing, directional transition state. |
| `src/mobile/screens/SettingsScreen.tsx` | Settings-owned Logs row callback. |
| `src/mobile/screens/LogsScreen.tsx` | Explicit back-to-Settings control. |
| `src/index.css` | Directional slide-and-fade keyframes plus reduced-motion fallback. |
| `scripts/check-mobile-ui.mjs` | Source-level regression assertions. |

### Task 1: Integrate Logs into Settings Navigation

**Files:**
- Modify: `src/mobile/MobileApp.tsx`
- Modify: `src/mobile/screens/SettingsScreen.tsx`
- Modify: `src/mobile/screens/LogsScreen.tsx`
- Test: `scripts/check-mobile-ui.mjs`

**Interfaces:**
- `SettingsScreen` receives `onOpenLogs: () => void`.
- `LogsScreen` receives `onBack: () => void`.

- [ ] **Step 1: Add failing source assertions**

Add assertions that the persistent `TABS` array has no Logs entry, Settings receives `onOpenLogs`, and Logs receives an `onBack` callback.

- [ ] **Step 2: Run red check**

Run: `node scripts/check-mobile-ui.mjs`

Expected: assertion failure because Logs is still a persistent tab.

- [ ] **Step 3: Implement minimal navigation changes**

- Remove `Terminal` and the Logs tab from `TABS`.
- Add local `logsOpen` state in `MobileApp`; Settings opens it and Logs closes it back to Settings.
- Render `LogsScreen` only while `activeTab === "settings" && logsOpen`.
- Add a button-style `SettingRow` equivalent in Settings that calls `onOpenLogs`.
- Add a Back button in Logs that calls `onBack`.

- [ ] **Step 4: Run green checks**

Run:

```powershell
node scripts/check-mobile-ui.mjs
npm run build
git diff --check
```

Expected: all commands succeed.

### Task 2: Add Directional Tab Transitions

**Files:**
- Modify: `src/mobile/MobileApp.tsx`
- Modify: `src/index.css`
- Test: `scripts/check-mobile-ui.mjs`

**Interfaces:**
- `MobileApp` derives transition direction from the prior and next persistent tab index.
- Swipe direction is passed directly to the transition class selection.

- [ ] **Step 1: Add failing source assertions**

Assert the CSS contains both forward/backward animation classes and a `prefers-reduced-motion: reduce` rule. Assert `MobileApp` applies a direction-derived animation class to the current content view.

- [ ] **Step 2: Run red check**

Run: `node scripts/check-mobile-ui.mjs`

Expected: assertion failure because transition classes are absent.

- [ ] **Step 3: Implement minimal animation**

- Track a `transitionDirection: "previous" | "next"` state in `MobileApp`.
- Route tab changes through a helper that derives direction from tab indexes for taps and accepts the gesture direction for swipes.
- Key the rendered content view by active tab/subview and apply either `mobile-view-enter-next` or `mobile-view-enter-previous`.
- Define 180–220ms translateX plus opacity animations in `src/index.css`; disable animation with `@media (prefers-reduced-motion: reduce)`.

- [ ] **Step 4: Run green checks**

Run:

```powershell
node scripts/check-mobile-ui.mjs
npm run build
git diff --check
```

Expected: all commands succeed.

### Task 3: Android Delivery Check

**Files:**
- Modify only for verification defects.

- [ ] **Step 1: Build APK**

Run the established ASCII Rustup/NDK Android build command:

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

Expected: `app-universal-debug.apk` is produced.

- [ ] **Step 2: Install and manually validate**

Install with `adb -s 3B15AV0166300000 install -r <apk>`. Check four bottom tabs, Settings → Logs → Settings, directional swipe/tap animations, and swipe protection inside controls and log scrolling.

- [ ] **Step 3: Commit selectively**

Stage only changed navigation-polish source, checker, spec, and plan files. Do not stage pre-existing Android work.
