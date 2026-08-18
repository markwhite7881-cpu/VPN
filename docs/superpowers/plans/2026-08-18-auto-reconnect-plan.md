# Automatic Server Reconnect and Pending-Settings Notice Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Automatically reconnect a running VPN when the user changes the selected server, while showing a persistent “Reconnect now” notice for other Config/Routing changes.

**Architecture:** Keep the existing `App.tsx` lifecycle as the single orchestration point. Add an explicit generic-settings change handler that records unapplied settings only while the VPN is running; keep `onSelectProfile` responsible for server-selection reconnects. Render the pending-settings notice in the shared app shell and reuse the existing `onStart`/`onStop` path for manual reconnects.

**Tech Stack:** React 18, TypeScript, Vitest, Vite, Tauri 2, existing `Button`/Card/design-system components.

## Global Constraints

- Preserve sing-box as primary and Xray as the strict capability fallback.
- Do not add a second engine-specific start path; reuse the existing lifecycle in `src/App.tsx`.
- Server selection while stopped must not start or stop the VPN.
- Server selection while running must use `stop -> regenerate -> start`.
- Config/Routing changes while running must not interrupt the VPN automatically.
- Config/Routing changes while stopped must not display a pending-settings notice.
- Keep pending-settings state separate from the existing error state.
- Do not expose raw Xray/sing-box errors, provider URLs, credentials, UUIDs, hostnames, runtime paths, or configuration contents to the WebView.
- Preserve Android behavior and UI; this plan is desktop-only.
- Do not change subscription naming, metadata exposure, updater/signing behavior, or release publication.
- Do not commit `tsconfig.tsbuildinfo` or generated build artifacts.
- Validate with frontend tests, production build, `git diff --check`, and an unsigned integration installer only after code validation.

---

## File Map

- Modify `src/App.tsx`:
  - own `reconnectRequired` state;
  - add a generic settings-change callback;
  - route Config/Routing/reset changes through that callback;
  - expose a reusable reconnect action;
  - clear pending state after successful server/manual reconnect;
  - render the shared pending-settings notice.
- Modify `src/components/ConfigTab.tsx`:
  - accept the explicit settings-change callback and use it for ConfigBuilder/reset actions.
- Modify `src/components/routing/RoutingTab.tsx`:
  - keep the existing settings prop shape but ensure all routing edits use the callback supplied by `App.tsx`.
- Modify `src/components/Card.tsx` or use existing shell markup only if necessary:
  - reuse current design-system surfaces; avoid adding a new component unless the existing shell cannot host the notice cleanly.
- Modify or create `src/App.test.tsx` only if the repository has a viable App-level test setup; otherwise extract pure orchestration helpers into a focused testable module.
- Create `src/lib/reconnectState.ts` only if needed to unit-test state transitions without mounting the full Tauri-dependent App. Prefer keeping the first implementation in `App.tsx` if tests can mock the existing API cleanly.
- Create `docs/superpowers/plans/2026-08-18-auto-reconnect-plan.md`:
  - this implementation plan.

## Task 1: Add explicit settings-change intent at the App boundary

**Files:**
- Modify: `src/App.tsx:323-338`, `src/App.tsx:884-918`
- Modify: `src/components/ConfigTab.tsx:20-105`
- Modify: `src/components/routing/RoutingTab.tsx:37-80`
- Test: `src/App.test.tsx` if viable, otherwise a focused helper test under `src/lib/`

**Interfaces:**
- `App.tsx` produces `handleSettingsChange(next: GeneratorSettings): void`.
- `ConfigTab` consumes `onSettingsChange: (next: GeneratorSettings) => void` and passes it to `ConfigBuilder`.
- `RoutingTab` consumes the same callback and passes it to all routing editor children through its existing `onSettingsChange` closure.

- [ ] **Step 1: Identify every generic settings mutation**

Search for all `setSettings`, `onSettingsChange`, and `onResetSettings` calls. Confirm that ConfigBuilder, RoutingTab, and reset-to-defaults are the only generic settings entry points. Do not classify server selection through this path.

Run:

```powershell
Set-Location 'C:\Users\Public\cwdev\cloakwire-v131-integration'
Select-String -Path 'src\**\*.tsx' -Pattern 'setSettings|onSettingsChange|onResetSettings' -Context 2,2
```

Expected: all generic mutations are accounted for before editing.

- [ ] **Step 2: Write failing state-transition tests**

Add tests for a pure helper if App mounting is not practical:

```ts
it('marks settings as pending only when the VPN is running', () => {
  expect(nextReconnectRequired(false, false)).toBe(false);
  expect(nextReconnectRequired(true, false)).toBe(true);
});

it('keeps pending settings after additional edits', () => {
  expect(nextReconnectRequired(true, true)).toBe(true);
});
```

If a helper is unnecessary, test the mounted App with mocked `api.getStatus`, `api.getLogs`, and Tauri internals. The assertions must distinguish stopped from running behavior.

- [ ] **Step 3: Implement the explicit handler**

Add state:

```ts
const [reconnectRequired, setReconnectRequired] = useState(false);
```

Add a callback:

```ts
const handleSettingsChange = useCallback((next: GeneratorSettings) => {
  setSettings(next);
  if (status.status === 'running') {
    setReconnectRequired(true);
  }
}, [status.status]);
```

Use this callback for Config, Routing, and reset-to-defaults. Keep the existing persistence effect on `settings`; do not add a second persistence mechanism.

- [ ] **Step 4: Run focused tests**

Run:

```powershell
Set-Location 'C:\Users\Public\cwdev\cloakwire-v131-integration'
npx vitest run src/App.test.tsx
```

Expected: the new stopped/running settings-change tests pass; unrelated tests remain green.

- [ ] **Step 5: Commit the boundary change**

```powershell
git add src/App.tsx src/components/ConfigTab.tsx src/components/routing/RoutingTab.tsx src/App.test.tsx src/lib/reconnectState.ts
git commit -m 'feat: track unapplied running settings'
```

Only include files that actually changed.

## Task 2: Centralize safe manual reconnect behavior

**Files:**
- Modify: `src/App.tsx:530-592`, `src/App.tsx:742-799`
- Test: App-level or helper tests covering reconnect success/failure

**Interfaces:**
- `App.tsx` produces a callback such as `reconnectCurrentProfile(): Promise<void>`.
- The callback uses the current `profiles`, `selectedIndex`, `settings`, and existing `onStartRef`/`onStart` path.
- The callback clears `reconnectRequired` only after successful start.

- [ ] **Step 1: Write failing reconnect tests**

Cover these behaviors:

```ts
it('does not clear pending settings before reconnect succeeds', async () => {
  api.startManaged.mockRejectedValueOnce(new Error('start failed'));
  await clickReconnectNow();
  expect(screen.getByText(/Reconnect the VPN to apply settings/i)).toBeVisible();
});

it('clears pending settings after a successful reconnect', async () => {
  api.startManaged.mockResolvedValueOnce(runningManagedResult);
  await clickReconnectNow();
  expect(screen.queryByText(/Reconnect the VPN to apply settings/i)).toBeNull();
});
```

Do not assert raw backend error text; assert the existing sanitized error presentation.

- [ ] **Step 2: Extract or reuse the existing stop/start sequence**

Refactor only enough to avoid duplicating the sequence. The reusable reconnect action must:

1. set busy and clear the current UI error;
2. clear the system proxy best effort;
3. stop the current process;
4. yield so the latest selected/settings React state is committed;
5. call the existing start flow;
6. reapply the system proxy through the existing sing-box-only condition;
7. clear `reconnectRequired` only after start succeeds;
8. preserve the pending notice and set the existing safe error if any step fails;
9. always clear busy in `finally`.

Do not call Xray `list_proxies`, `select_proxy`, or `test_delay` as part of this feature.

- [ ] **Step 3: Route server selection through the shared reconnect action**

Keep the special server-selection semantics:

- update `selectedIndex` and `default_outbound` first;
- if the selected profile is a ready subscription profile, retain the existing behavior and do not introduce a new backend path;
- if the VPN is running, invoke the shared reconnect action after the state update;
- if the VPN is stopped, do not invoke reconnect;
- clear `reconnectRequired` only after successful reconnect.

Avoid stale-closure bugs by using the existing `onStartRef` pattern or a callback that receives the latest selection explicitly.

- [ ] **Step 4: Run focused reconnect tests**

```powershell
Set-Location 'C:\Users\Public\cwdev\cloakwire-v131-integration'
npx vitest run src/App.test.tsx src/components/HomeTab.test.tsx
```

Expected: stopped selection does not call stop/start; running selection does; failed reconnect preserves pending state; successful reconnect clears it.

- [ ] **Step 5: Commit reconnect orchestration**

```powershell
git add src/App.tsx src/App.test.tsx src/lib/reconnectState.ts
 git commit -m 'feat: centralize safe reconnect flow'
```

## Task 3: Add the shared pending-settings notice

**Files:**
- Modify: `src/App.tsx` near the shared error/status presentation and reconnect callback
- Modify: existing `src/components/Button.tsx` only if its variants cannot support the action
- Test: App-level UI test

**Interfaces:**
- The App shell consumes `reconnectRequired`, `status.status`, `busy`, and `reconnectCurrentProfile`.
- The notice is visible only when `reconnectRequired && status.status === 'running'`.

- [ ] **Step 1: Write the failing UI test**

```ts
it('shows one persistent reconnect notice for running unapplied settings', async () => {
  render(<App />);
  await changeRoutingSetting();
  expect(screen.getByText(/Settings saved/i)).toBeVisible();
  expect(screen.getByRole('button', { name: /Reconnect now/i })).toBeVisible();
  expect(screen.getAllByText(/Settings saved/i)).toHaveLength(1);
});
```

Also verify that the notice is absent when the mocked status is stopped.

- [ ] **Step 2: Implement the notice with existing design primitives**

Render it near the existing error/status area in the shared App shell, using existing card/border/text/button classes. The notice must:

- be informational, not destructive;
- remain visible while more settings are changed;
- show the action as busy/disabled during reconnect;
- disappear after successful reconnect;
- remain visible after failure;
- not be shown while stopped.

Use user-facing copy consistent with the approved design:

```text
Settings saved. Reconnect the VPN to apply changes.
Reconnect now
```

Do not add raw error details to this notice.

- [ ] **Step 3: Run UI tests**

```powershell
Set-Location 'C:\Users\Public\cwdev\cloakwire-v131-integration'
npx vitest run src/App.test.tsx
```

Expected: notice visibility, single-instance behavior, action state, and stopped-state hiding pass.

- [ ] **Step 4: Commit the notice**

```powershell
git add src/App.tsx src/components/Button.tsx src/App.test.tsx
 git commit -m 'feat: add pending settings reconnect notice'
```

## Task 4: Regression coverage for no-op triggers and final frontend validation

**Files:**
- Modify: `src/App.test.tsx` and/or focused helper tests
- Modify: `src/components/routing/*.test.tsx` only if existing test structure requires it

**Interfaces:**
- Tests verify the public behavior without relying on runtime paths, provider metadata, or raw configuration contents.

- [ ] **Step 1: Add the remaining regression tests**

Cover:

```ts
it('keeps a single pending notice after multiple settings edits', async () => {
  await changeRoutingSetting();
  await changeAnotherRoutingSetting();
  expect(screen.getAllByRole('button', { name: /Reconnect now/i })).toHaveLength(1);
});

it('does not mark initial settings hydration as pending', async () => {
  render(<App />);
  await waitForAppHydration();
  expect(screen.queryByText(/Settings saved/i)).toBeNull();
});

it('does not reconnect merely by switching tabs', async () => {
  render(<App />);
  await clickTab('Routing');
  await clickTab('Home');
  expect(api.stop).not.toHaveBeenCalled();
  expect(api.startManaged).not.toHaveBeenCalled();
});

it('clears a prior pending notice after a successful server reconnect', async () => {
  await changeRoutingSetting();
  await selectDifferentServer();
  await waitForReconnectSuccess();
  expect(screen.queryByText(/Settings saved/i)).toBeNull();
});
```

Use test doubles for Tauri API calls. Never put real subscription URLs, UUIDs, hosts, or credentials in fixtures.

- [ ] **Step 2: Run the complete frontend test suite**

```powershell
Set-Location 'C:\Users\Public\cwdev\cloakwire-v131-integration'
npm run test -- --run
```

Expected: all test files pass with zero failures.

- [ ] **Step 3: Run production build with the required manifest setting**

```powershell
Set-Location 'C:\Users\Public\cwdev\cloakwire-v131-integration'
$env:CLOAKWIRE_TEST_MANIFEST = ''
npm run build
```

Expected: TypeScript and Vite build pass. The existing chunk-size warning is acceptable if unchanged.

- [ ] **Step 4: Check formatting and generated-file hygiene**

```powershell
Set-Location 'C:\Users\Public\cwdev\cloakwire-v131-integration'
git diff --check
git status --short
```

Restore `tsconfig.tsbuildinfo` if generated. The only tracked changes should be intentional source/tests/docs.

- [ ] **Step 5: Commit final regression coverage**

```powershell
git add src/App.test.tsx src/components/routing
 git commit -m 'test: cover reconnect notice behavior'
```

## Task 5: Build and verify the unsigned Windows test installer

**Files:**
- No source changes expected.
- Build output: `C:\Users\Public\cwdev\cw-v131-integration-target-reconnect\`

**Interfaces:**
- Consumes the validated integration worktree and local ignored sidecars.
- Produces an unsigned NSIS installer for local validation only.

- [ ] **Step 1: Build the NSIS installer with explicit Cargo and manifest environment**

```powershell
Set-Location 'C:\Users\Public\cwdev\cloakwire-v131-integration'
$env:CLOAKWIRE_TEST_MANIFEST = ''
$env:CARGO_TARGET_DIR = 'C:\Users\Public\cwdev\cw-v131-integration-target-reconnect'
$env:Path = 'C:\Users\Алексей\.cargo\bin;' + $env:Path
npm run tauri build -- --bundles nsis
```

Expected: one unsigned installer at:

```text
C:\Users\Public\cw-v131-integration-target-reconnect\release\bundle\nsis\Cloakwire_1.3.0_x64-setup.exe
```

- [ ] **Step 2: Verify artifact and worktree**

```powershell
$artifact = 'C:\Users\Public\cw-v131-integration-target-reconnect\release\bundle\nsis\Cloakwire_1.3.0_x64-setup.exe'
$item = Get-Item -LiteralPath $artifact
Get-FileHash -LiteralPath $artifact -Algorithm SHA256
$item.Length
git status --short
```

Expected: artifact exists, size/hash are recorded, and the worktree is clean after restoring generated `tsconfig.tsbuildinfo`.

- [ ] **Step 3: Manually validate the user flows**

Using a local test environment only:

1. Start with VPN stopped; change a routing setting; confirm no notice and no reconnect.
2. Connect; change a routing setting; confirm the notice appears and traffic remains connected until action.
3. Click “Reconnect now”; confirm status transitions through stopping/starting and the notice disappears after success.
4. While connected, select another server; confirm automatic reconnect and the selected server is active afterward.
5. While stopped, select another server; confirm no automatic connection.
6. Change several settings; confirm only one notice is shown.
7. Force a reconnect failure in a test environment; confirm the notice remains and only the existing safe error is shown.

## Completion Criteria

- All approved behavior in `docs/superpowers/specs/2026-08-18-auto-reconnect-design.md` is implemented.
- Full frontend test suite passes.
- Production build passes with `CLOAKWIRE_TEST_MANIFEST` empty.
- `git diff --check` passes.
- No generated files or secrets are committed.
- Unsigned NSIS installer exists and its size/SHA-256 are recorded.
- Manual server-selection and pending-settings flows are validated without changing Android or release publication scope.
