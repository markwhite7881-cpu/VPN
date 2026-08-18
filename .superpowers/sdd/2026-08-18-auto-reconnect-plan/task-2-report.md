# Task 2 implementation report — Centralize safe manual reconnect behavior

## Changed files

- `src/App.tsx`
  - Changed `onStart` to return an explicit `Promise<boolean>` success result while preserving its existing managed-start, refresh, error sanitization, and sing-box-only system-proxy behavior.
  - Added `reconnectCurrentProfile()` as the shared stop → yield → start orchestration path.
  - The shared path clears the system proxy best-effort, stops the current process, yields for React state commitment, invokes the latest `onStart` through `onStartRef`, clears `reconnectRequired` only after a successful start, preserves pending state on failure, and always clears busy state.
  - Replaced the running-profile-selection inline stop/start sequence with the shared reconnect callback.
  - Preserved ready subscription profile handling, stopped-selection behavior, and the existing Xray capability boundary. No Xray proxy capability calls were added.

## Validation commands and results

- `npm.cmd test -- --run src/lib/reconnectState.test.ts src/lib/systemProxy.test.ts`
  - Passed: 2 test files, 6 tests.
- `npm.cmd test`
  - Passed: 6 test files, 20 tests.
- `npm.cmd run build`
  - Passed: TypeScript build and Vite production build completed successfully.
  - Existing Vite chunk-size warning remains; it is unrelated to this task.
- `git diff --check`
  - Passed with no whitespace errors.

## Assumptions

- `onStart` success means the managed process start path completed successfully. A best-effort system-proxy application failure remains a non-fatal start outcome, matching the existing behavior.
- The existing `onStartRef` plus a microtask yield is the intended mechanism for reading the latest selected profile/settings after selection state updates.
- Existing App-level test setup is not present; validation used the repository's focused reconnect/system-proxy tests plus the full suite and build.

## Blockers

- None.
