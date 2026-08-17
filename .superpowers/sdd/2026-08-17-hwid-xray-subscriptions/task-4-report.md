# Task 4 Report — Backend-Owned Link Execution and Safe Frontend State

## Result
Implemented and committed the amended Task 4 backend-first opaque link-list route and frontend migration.

## RED/GREEN evidence

### RED
- Before implementation, the existing frontend flattened `useSubscriptions().lastResult` raw `Outbound[]` into `App.tsx` profiles and generated configs in React.
- Before adding the migration test, `npm test` reported `No test files found, exiting with code 1`.
- Before aligning UI types, `npm run build` failed because backend `SubscriptionSummary[]` was being passed to URL-bearing legacy `Subscription[]` props.

### GREEN
- Added focused Vitest migration tests proving failed backend migration leaves `singbox-client.subscriptions.v1` untouched and successful migration removes it only after resolution.
- `npm test`: PASS — 1 file, 4 tests.
- `npm run build`: PASS — TypeScript and Vite production build completed; only the existing chunk-size warning.
- `git diff --check`: PASS.

## Changes

### Backend
- Added opaque `SubscriptionLinkRef` DTO.
- Added private service resolution for explicit refs and all link-list refs.
- Validation rejects missing subscription IDs, non-link-list records, malformed keys, stale indexes, and duplicate selections with fixed safe `AppError` categories/messages.
- Added `start_managed_singbox` command. It accepts manual `Outbound` values plus opaque subscription refs/settings, resolves subscription outbounds in Rust, merges them, builds the existing sing-box config, writes the managed config, and starts through the existing `ProcessManager`/controller lifecycle.
- Registered the command in Tauri invoke handlers.
- Added focused resolver test coverage.

### Frontend
- Added safe subscription DTO mirrors and managed-launch DTO/API wrappers.
- Rewrote `useSubscriptions` around backend snapshots, backend refresh/add/remove/interval/child commands, one-time legacy migration, and no subscription persistence after migration.
- Removed subscription `Outbound[]` flattening from `App.tsx`; manual profiles remain in the existing frontend path, and startup calls `start_managed_singbox` with opaque link references.
- Removed subscription URL rendering from `SubscriptionsCard` and aligned it to safe summaries/metadata/error fields.
- Added Vitest scripts and `vitest ^2.1.9` dependency.

## Validation commands/results

- `npm install --package-lock-only --ignore-scripts`: PASS.
- `npm test`: PASS (4/4).
- `npm run build`: PASS (only the existing Vite chunk-size warning).
- `git diff --check`: PASS.
- `cargo fmt --manifest-path src-tauri\\Cargo.toml -- --check`: BLOCKED; exact probe output: `CARGO_UNAVAILABLE` (Rust toolchain is unavailable on PATH in this environment).
- Focused Rust tests (`subscriptions::store::tests` and `managed_launch_tests`): BLOCKED by the same missing `cargo` executable.

## Design decisions
- Subscription records retain raw parsed `Outbound` values only in Rust storage/service memory.
- React receives only positional link keys, generic labels, allowlisted protocol, provider-safe metadata, and safe error DTOs.
- Automatic/all link-list selection is backend-owned; explicit selection is represented only by `{ subscription_id, link_key }`.
- Manual profile storage and behavior remain unchanged; manual profiles are accepted by the managed command as an existing executable input.
- No Xray runtime, ready-config execution, packaging, Android work, or manual-profile storage migration was added.

## Commit
- `fc0ea77 feat: route subscription links through backend`

## Concerns / remaining risks
- Rust compilation and focused Rust tests could not be executed because Cargo/Rust is not installed or exposed in the current environment. The Rust edits were kept narrow and follow existing APIs, but should be compiled and formatted in a Rust-enabled environment before merge.
- The existing UI still permits entering a subscription URL in the add form because it is required as command input; URLs are not persisted/rendered after backend migration. Full end-to-end Tauri migration behavior should be exercised in the desktop shell.
- Existing unrelated full Rust package/UAC harness defects were not evaluated because Cargo was unavailable.

## Verifier and persistence-boundary follow-up
- Restored the original `start_singbox` command signature/body and preserved registration.
- Removed the obsolete `fetch_subscription` command and Tauri registration; remaining `fetch_legacy_links` references are internal Rust tests/service only.
- Made `SubscriptionRecord` and `ChildProfileRecord` deserialization-only command-facing types and added redacted `Debug` implementations. Private `StoredSubscriptionRecord` / `StoredChildProfileRecord` DTOs now own JSON persistence, preserving legacy records without allowing raw fields to serialize through command-facing models.
- Added a persistence round-trip test for private bundle child configuration data.
- Added backend-only `deduplicate_outbounds`, which hashes serialized outbounds in Rust immediately before managed config generation and retains first-seen order; digests and raw values are never returned or logged. Added focused coverage for manual/subscription duplicates.
- Added `ConnectionProfile` frontend union. Manual outbounds retain existing behavior, while subscription entries carry only an opaque reference, label, and protocol. GeoIP, latency, config, and routing hooks receive manual profiles only.
- `managedSelectionForProfile` sends all backend links for Auto, exactly one opaque ref for an explicit subscription, and no subscription ref for a manual selection. Focused tests cover manual-first ordering, both selection modes, and legacy localStorage migration success/failure.
- Fresh verifier-round validation: `npm test` PASS (4/4), `npm run build` PASS (existing Vite chunk-size warning only), and `git diff --check` PASS. Rust formatting and focused tests remain blocked because the Cargo probe returned `CARGO_UNAVAILABLE`.
