# Task 3 — Implementation report

## Changed files

- `src/App.tsx`
  - Added one shared App-shell informational notice gated by `reconnectRequired && status.status === "running"`.
  - Added the exact copy `Settings saved. Reconnect the VPN to apply changes.` and `Reconnect now`.
  - Reused the existing `Button` component with the `outline` variant.
  - The action calls the existing `reconnectCurrentProfile` callback and follows `busy` via `disabled` and `aria-busy`.
  - No reconnect orchestration semantics were changed.
- `.superpowers/sdd/2026-08-18-auto-reconnect-plan/task-3-report.md`
  - Added this report.

`src/components/Button.tsx` was not changed because the existing variants support the action.

## Validation

- `npm.cmd test`
  - Passed: 6 test files, 20 tests.
- `npm.cmd run build`
  - Passed: TypeScript build and Vite production build completed successfully.
  - Existing Vite warning remains: the main JavaScript chunk is larger than 500 kB after minification.

There is no practical App-level test harness in the repository: no `src/App.test.*`, no React Testing Library setup, and the existing tests are focused unit/component tests. No brittle App-level test was added; focused coverage remains for the later Task 4 work as requested.

## Assumptions

- The existing `busy` state represents reconnect work for the shared action, so disabling the button while `busy` is true is the intended loading behavior.
- The existing `reconnectCurrentProfile` callback owns pending-state clearing after successful reconnect; the notice therefore disappears through the existing state transition without additional orchestration.
- The informational styling uses existing design-system tokens (`border-primary`, `bg-primary`, `bg-background`/foreground tokens) and does not expose runtime or backend details.

## Blockers / concerns

- No blockers for this task.
- The working tree already contained a modified `tsconfig.tsbuildinfo` before this task; it was not changed or included in the task commit.
- The production build emits the pre-existing chunk-size warning noted above; it does not fail the build.
