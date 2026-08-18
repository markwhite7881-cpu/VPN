# Home Xray Telemetry Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add safe Xray traffic, country, and latency telemetry to Home, make the connected power button semantic green, and preserve existing sing-box, routing, subscription, and updater behavior.

**Architecture:** Keep the existing frontend `traffic` event and `TrafficSample` contract. Add a Rust-owned Xray StatsService collector that polls loopback-only gRPC counters and shares the existing cumulative-counter sampler with sing-box. Add a Rust-only ready-profile metadata command that returns only country code and bounded latency, then let Home render both engines through one presentation path. Xray updater work remains excluded from this plan.

**Tech Stack:** Rust 1.77-compatible Tauri 2 backend, Tokio, tonic/prost-generated Xray StatsService client, serde_json, reqwest, React 18 + TypeScript, existing Tailwind/shadcn-style semantic tokens, Vitest, Rust unit tests.

## Global Constraints

- Preserve Android functionality/UI byte-for-byte where possible; this feature changes desktop Xray/Home behavior only and must not alter Android runtime behavior.
- Preserve existing subscriptions, manual profiles, profile selection, Routing 2.0, TUN/system-proxy behavior, logs, and updater behavior.
- sing-box remains the automatic primary engine; Xray remains a strict capability fallback.
- Do not expose raw Xray errors, provider URLs, profile contents, UUIDs, hostnames, credentials, runtime paths, or process paths to the WebView.
- Xray receives runtime-only environment/config data from Rust; no parent-process environment mutation.
- Local Xray StatsService binds to `127.0.0.1` on a dynamically allocated port and never crosses the Tauri command boundary.
- Existing Xray HTTP inbound behavior must remain provider-compatible: preserve an existing tagged inbound tag; assign `cloakwire-managed-http` only when the selected runtime HTTP inbound has no tag; reject reserved-tag collisions.
- Failed Xray telemetry or metadata probing must not stop an otherwise valid Xray connection.
- Do not add an Xray updater to the Updates card in this plan.
- Match the existing semantic design system; do not introduce arbitrary hard-coded palette colors.
- Do not commit keys, credentials, tokens, APK/AAR outputs, keystores, `target/`, `node_modules`, generated Android directories, release staging, Xray executables, downloaded archives, raw subscription bundles, or credential-bearing fixtures.

---

## File Map

### Backend files

- Modify `src-tauri/Cargo.toml`: add Rust 1.77-compatible tonic/prost dependencies and build tooling.
- Modify `src-tauri/build.rs`: generate a client-only Rust module from the vendored Xray StatsService proto.
- Create `src-tauri/proto/xray_stats.proto`: repository-owned minimal official StatsService contract containing `GetStats`.
- Modify `src-tauri/src/lib.rs`: register the new `xray::stats` module if required by the final module layout.
- Modify `src-tauri/src/xray/inbound.rs`: return the actual traffic-bearing inbound tag and assign the reserved tag only for an untagged selected inbound.
- Create `src-tauri/src/xray/stats.rs`: merge loopback StatsService config, query cumulative counters, and own the Xray collector task.
- Modify `src-tauri/src/xray/mod.rs`: include the Xray stats runtime specification in prepared internal launch data without serializing it to the frontend.
- Modify `src-tauri/src/engine/mod.rs`: add an internal optional Xray telemetry launch specification to `LaunchSpec`.
- Modify `src-tauri/src/traffic.rs`: extract and test source-neutral cumulative-counter-to-`TrafficSample` conversion; keep sing-box WebSocket behavior unchanged.
- Modify `src-tauri/src/process.rs`: start/stop Xray telemetry by launch generation alongside the existing sing-box stream and cancel it on stop, crash, reset, or replacement.
- Modify `src-tauri/src/commands.rs`: return safe ready-profile metadata and pass private Xray telemetry launch data into `LaunchSpec`.
- Modify `src-tauri/src/lib.rs`: register the new safe metadata command.
- Create or modify `src-tauri/src/xray/presentation.rs`: extract only safe endpoint/country/latency inputs from a resolved Xray config and sanitize the result.

### Frontend files

- Modify `src/lib/types.ts`: add `HomeProfileMetadata` and an opaque ready-profile metadata request/result type.
- Modify `src/lib/api.ts`: add the typed `getReadyProfileMetadata` wrapper; do not add any endpoint-bearing API.
- Create `src/hooks/useReadyProfileMetadata.ts`: cache metadata by opaque subscription/child reference and refresh only relevant Xray ready profiles.
- Modify `src/components/HomeTab.tsx`: use safe metadata for ready profiles, keep manual sing-box probing unchanged, and apply green connected-state button tokens.
- Modify `src/App.tsx`: pass the current ready-profile metadata map into `HomeTab`; keep start/stop/system-proxy paths unchanged.
- Modify/add focused frontend tests near `src/lib/systemProxy.test.ts` or create `src/hooks/useReadyProfileMetadata.test.ts` for cache/fallback behavior.

### Verification and documentation

- Add Rust unit tests beside `traffic.rs`, `xray/inbound.rs`, `xray/stats.rs`, and `xray/presentation.rs`.
- Add/update frontend tests for metadata rendering and connected button state.
- Update `docs/superpowers/plans/` only with this implementation plan; do not modify the approved design spec unless implementation discovers a contradiction that requires user review.

---

## Task 1: Make the managed inbound expose a safe traffic tag

**Files:**
- Modify: `src-tauri/src/xray/inbound.rs:7-13, 59-89`
- Test: `src-tauri/src/xray/inbound.rs` module tests

**Interfaces:**
- Consumes: provider Xray JSON and the existing port allocator.
- Produces: `ManagedHttpInbound { value, proxy_host, proxy_port, traffic_tag, injected }`, where `traffic_tag: String` is backend-internal and never serialized.

- [ ] **Step 1: Write failing tests for tag preservation and untagged assignment**

Add tests with these exact cases:

```rust
#[test]
fn preserves_existing_http_inbound_tag_for_stats() {
    let result = ensure_managed_http_inbound(
        json!({"inbounds":[{"tag":"provider-http","listen":"127.0.0.1","port":10809,"protocol":"http"}]}),
        || Ok(20809),
    ).unwrap();

    assert_eq!(result.traffic_tag, "provider-http");
    assert_eq!(result.value["inbounds"][0]["tag"], "provider-http");
}

#[test]
fn assigns_reserved_tag_to_untagged_runtime_inbound() {
    let result = ensure_managed_http_inbound(
        json!({"inbounds":[{"listen":"127.0.0.1","port":10809,"protocol":"http"}]}),
        || Ok(20809),
    ).unwrap();

    assert_eq!(result.traffic_tag, MANAGED_HTTP_TAG);
    assert_eq!(result.value["inbounds"][0]["tag"], MANAGED_HTTP_TAG);
}
```

- [ ] **Step 2: Run the focused Rust tests and confirm failure**

Run from `C:\Users\Public\cwdev\cloakwire-hwid-xray`:

```powershell
$env:PATH = 'C:\Users\Public\cwdev\rustup-home\toolchains\stable-x86_64-pc-windows-msvc\bin;' + $env:PATH
$env:CLOAKWIRE_TEST_MANIFEST = '1'
cargo test --manifest-path src-tauri\Cargo.toml xray::inbound --lib
```

Expected: FAIL because `ManagedHttpInbound` has no `traffic_tag` and untagged inputs currently return without a tag.

- [ ] **Step 3: Implement the minimal runtime-only tag behavior**

Change only the cloned runtime JSON:

- keep `MANAGED_HTTP_TAG` collision rejection;
- when exactly one HTTP inbound is selected and it has a non-empty tag, return that tag unchanged;
- when exactly one selected HTTP inbound has no tag, insert `MANAGED_HTTP_TAG` into that object and return it;
- when no HTTP inbound exists, inject the current managed inbound and return `MANAGED_HTTP_TAG`;
- preserve the selected inbound’s listen address and port.

- [ ] **Step 4: Run the focused tests and the existing inbound regression suite**

```powershell
cargo test --manifest-path src-tauri\Cargo.toml xray::inbound --lib
```

Expected: PASS, including the existing public-listener, ambiguous-inbound, invalid-port, and reserved-tag tests.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/xray/inbound.rs
git commit -m 'feat: expose safe Xray traffic inbound tag'
```

---

## Task 2: Add the Xray StatsService client and config merge

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/build.rs`
- Create: `src-tauri/proto/xray_stats.proto`
- Create: `src-tauri/src/xray/stats.rs`
- Modify: `src-tauri/src/xray/mod.rs`
- Test: `src-tauri/src/xray/stats.rs` module tests

**Interfaces:**
- Consumes: prepared provider config, `traffic_tag`, and a Rust-owned StatsService port.
- Produces: `XrayStatsSpec { api_host, api_port, traffic_tag }`, runtime JSON merge, and a private `XrayStatsStream` that emits `TrafficSample`.

- [ ] **Step 1: Add the official minimal protobuf contract and failing config tests**

Create `src-tauri/proto/xray_stats.proto` with package `xray.app.stats.command`, messages `GetStatsRequest`, `Stat`, `GetStatsResponse`, and the client-only `StatsService.GetStats` RPC.

Add failing tests for:

```rust
#[test]
fn merge_adds_loopback_stats_api_and_inbound_counters() {
    let (value, spec) = merge_stats_config(json!({
        "inbounds": [{"tag":"provider-http","listen":"127.0.0.1","port":10809,"protocol":"http"}],
        "outbounds": [{"tag":"proxy","protocol":"freedom"}]
    }), "provider-http", || Ok(29001)).unwrap();

    assert_eq!(value["stats"], json!({}));
    assert_eq!(value["api"]["listen"], "127.0.0.1:29001");
    assert_eq!(value["api"]["services"], json!(["StatsService"]));
    assert_eq!(value["policy"]["system"]["statsInboundUplink"], true);
    assert_eq!(value["policy"]["system"]["statsInboundDownlink"], true);
    assert_eq!(spec.traffic_tag, "provider-http");
}

#[test]
fn merge_preserves_existing_api_services_and_policy_fields() {
    let (value, _) = merge_stats_config(json!({
        "api": {"tag":"provider-api","listen":"127.0.0.1:9000","services":["HandlerService"]},
        "policy": {"levels":{"0":{"handshake":4}},"system":{"statsInboundUplink":false}},
        "stats": {"existing": true}
    }), "provider-http", || Ok(29001)).unwrap();

    assert_eq!(value["api"]["services"], json!(["HandlerService", "StatsService"]));
    assert_eq!(value["policy"]["levels"]["0"]["handshake"], 4);
    assert_eq!(value["policy"]["system"]["statsInboundUplink"], true);
    assert_eq!(value["stats"]["existing"], true);
}
```

- [ ] **Step 2: Run the focused tests to confirm failure**

```powershell
cargo test --manifest-path src-tauri\Cargo.toml xray::stats --lib
```

Expected: FAIL because the module and merge function do not exist.

- [ ] **Step 3: Add pinned gRPC dependencies and generated client build**

Add versions compatible with the repository’s Rust 1.77 floor, using one consistent tonic/prost release line:

```toml
[dependencies]
tonic = { version = "0.12", default-features = false, features = ["transport"] }
prost = "0.13"

[build-dependencies]
tonic-build = "0.12"
```

Update `src-tauri/build.rs` to generate client-only code from `proto/xray_stats.proto`. If the Windows build environment lacks `protoc`, use a vendored compiler dependency rather than requiring a machine-global install, and document the exact build dependency in `Cargo.toml`.

- [ ] **Step 4: Implement `merge_stats_config` with field-specific merging**

Use a second loopback-only API listener with a dynamically allocated port. Configure:

```json
{
  "api": {
    "listen": "127.0.0.1:<allocated-port>",
    "services": ["StatsService"]
  },
  "stats": {},
  "policy": {
    "system": {
      "statsInboundUplink": true,
      "statsInboundDownlink": true
    }
  }
}
```

Requirements:

- preserve existing `stats` object fields;
- preserve existing `policy.levels` and `policy.system` fields;
- append `StatsService` exactly once;
- reject an existing non-loopback API listener rather than weakening it;
- reject or safely replace a conflicting API listen address in the runtime clone only after a focused test proves provider API behavior is not lost;
- never log or return the allocated API address.

Build counter names from the actual inbound tag:

```text
inbound>>><traffic_tag>>>traffic>>>uplink
inbound>>><traffic_tag>>>traffic>>>downlink
```

- [ ] **Step 5: Run config merge and existing Xray tests**

```powershell
cargo fmt --manifest-path src-tauri\Cargo.toml -- --check
cargo test --manifest-path src-tauri\Cargo.toml xray::stats --lib
cargo test --manifest-path src-tauri\Cargo.toml xray --lib
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add src-tauri\Cargo.toml src-tauri\build.rs src-tauri\proto\xray_stats.proto src-tauri\src\xray\stats.rs src-tauri\src\xray\mod.rs
git commit -m 'feat: configure Xray StatsService telemetry'
```

---

## Task 3: Extract shared counter sampling and implement the Xray collector

**Files:**
- Modify: `src-tauri/src/traffic.rs:23-31, 167-205`
- Modify: `src-tauri/src/xray/stats.rs`
- Test: `src-tauri/src/traffic.rs` and `src-tauri/src/xray/stats.rs` tests

**Interfaces:**
- Consumes: cumulative uplink/downlink counters and elapsed time.
- Produces: `TrafficSample` with first-sample zero rates, cumulative totals, saturating reset handling, and `ts_ms`.

- [ ] **Step 1: Add failing shared sampler tests**

Extract a `CounterSampler` with tests for:

```rust
#[test]
fn first_sample_has_zero_rate_and_preserves_totals() {}

#[test]
fn second_sample_uses_elapsed_time_for_rates() {}

#[test]
fn counter_decrease_is_treated_as_reset() {}

#[test]
fn malformed_or_missing_stat_is_reported_without_panicking() {}
```

Use deterministic elapsed durations in the sampler tests instead of sleeping. Keep one existing integration-style sleep test only if it still adds value after extraction.

- [ ] **Step 2: Run the focused sampler tests and confirm failure**

```powershell
cargo test --manifest-path src-tauri\Cargo.toml traffic::tests --lib
```

Expected: FAIL for the new sampler symbols/tests.

- [ ] **Step 3: Implement the sampler and refactor sing-box parsing to use it**

Keep `parse_traffic_frame` behavior unchanged from the user’s perspective. Move only the cumulative-to-rate math into the shared sampler so sing-box remains a regression baseline.

- [ ] **Step 4: Implement `XrayStatsStream`**

Use the generated tonic client to connect to the Rust-private `127.0.0.1:<stats_port>` endpoint. Poll both counter names once per second with `reset = false`, feed values to `CounterSampler`, and emit `TrafficSample::EVENT` through `AppHandle`.

Rules:

- transient `GetStats` “not found” during startup is retried with bounded backoff;
- API connection failure is retried without flooding logs;
- a malformed/negative counter is ignored or clamped to zero;
- failures produce sanitized logs only;
- no endpoint, counter name, provider tag, or raw gRPC error is emitted to the WebView;
- `stop()` cancels the task and aborts its join handle like the existing stream.

- [ ] **Step 5: Run sampler, stats, and existing traffic tests**

```powershell
cargo test --manifest-path src-tauri\Cargo.toml traffic --lib
cargo test --manifest-path src-tauri\Cargo.toml xray::stats --lib
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add src-tauri\src\traffic.rs src-tauri\src\xray\stats.rs
git commit -m 'feat: stream Xray traffic counters'
```

---

## Task 4: Own Xray telemetry through the process lifecycle

**Files:**
- Modify: `src-tauri/src/engine/mod.rs`
- Modify: `src-tauri/src/xray/mod.rs`
- Modify: `src-tauri/src/commands.rs:178-244`
- Modify: `src-tauri/src/process.rs:96-123, 319-344, 376-401, 519-537, 558-571`
- Test: `src-tauri/src/process.rs` lifecycle tests and Xray preparation tests

**Interfaces:**
- Consumes: `LaunchSpec` with optional private `XrayStatsSpec`.
- Produces: exactly one telemetry task owned by the active `run_id`; no stale task after stop, crash, reset, engine switch, or failed launch.

- [ ] **Step 1: Add failing lifecycle tests**

Add tests that use a test emitter/counter and assert:

```rust
#[tokio::test]
async fn xray_launch_starts_one_telemetry_owner() {}

#[tokio::test]
async fn stopping_xray_cancels_telemetry_before_process_state_is_cleared() {}

#[tokio::test]
async fn replacing_xray_with_singbox_cannot_emit_from_old_xray_run() {}

#[tokio::test]
async fn failed_xray_spawn_does_not_leave_telemetry_running() {}
```

The tests must assert ownership by `run_id`, not timing alone.

- [ ] **Step 2: Run focused lifecycle tests and confirm failure**

```powershell
$env:PATH = 'C:\Users\Public\cwdev\rustup-home\toolchains\stable-x86_64-pc-windows-msvc\bin;' + $env:PATH
$env:CLOAKWIRE_TEST_MANIFEST = '1'
cargo test --manifest-path src-tauri\Cargo.toml process --lib
```

Expected: FAIL because `LaunchSpec` and `ProcessManager` have no Xray telemetry owner.

- [ ] **Step 3: Add private launch data and lifecycle ownership**

Add an internal optional Xray telemetry field to `LaunchSpec`. In `start_ready_profile_inner`, use the result of runtime preparation to populate it. Do not add it to any serialized command result.

In `ProcessManager`:

- keep sing-box `TrafficStream` unchanged;
- add a private optional `XrayStatsStream` handle;
- start it only after the Xray child is successfully spawned and status is marked running;
- stop it before Xray process teardown completes;
- stop it unconditionally in reset and finalize paths;
- check `run_id` before each emit and before replacing the stored task;
- ensure a failed Xray validation/spawn clears private telemetry state.

- [ ] **Step 4: Run lifecycle tests and all backend tests**

```powershell
cargo fmt --manifest-path src-tauri\Cargo.toml -- --check
cargo test --manifest-path src-tauri\Cargo.toml process --lib
cargo test --manifest-path src-tauri\Cargo.toml --lib
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri\src\engine\mod.rs src-tauri\src\xray\mod.rs src-tauri\src\commands.rs src-tauri\src\process.rs
git commit -m 'feat: own Xray telemetry through process lifecycle'
```

---

## Task 5: Add safe Xray ready-profile country and latency metadata

**Files:**
- Create: `src-tauri/src/xray/presentation.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/subscriptions/service.rs` only if a private resolver helper is needed
- Modify: `src/lib/types.ts`
- Modify: `src/lib/api.ts`
- Create: `src/hooks/useReadyProfileMetadata.ts`
- Test: `src-tauri/src/xray/presentation.rs`, command tests, and frontend hook tests

**Interfaces:**
- Consumes: opaque `{ subscription_id, child_key }` from the frontend.
- Produces: `{ country_code: string | null, latency_ms: number | null }` only.

- [ ] **Step 1: Add failing backend privacy and extraction tests**

Test supported Xray outbound shapes used by the repository (VLESS, VMess, Trojan, Shadowsocks, and compatible nested provider outbounds) and assert:

```rust
#[test]
fn safe_metadata_contains_only_country_and_latency() {}

#[test]
fn unsupported_or_missing_endpoint_returns_null_metadata() {}

#[test]
fn serialized_metadata_cannot_contain_host_port_url_uuid_or_raw_config() {}
```

The test fixtures must use synthetic non-credential endpoints and secrets, and the assertions must inspect serialized output and debug formatting for forbidden fields.

- [ ] **Step 2: Run focused tests and confirm failure**

```powershell
cargo test --manifest-path src-tauri\Cargo.toml xray::presentation --lib
```

Expected: FAIL because the safe presentation module and command do not exist.

- [ ] **Step 3: Implement backend-owned metadata resolution**

Add:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct HomeProfileMetadata {
    pub country_code: Option<String>,
    pub latency_ms: Option<u32>,
}
```

Implement a Tauri command that accepts only opaque subscription identifiers, resolves the child through `SubscriptionService`, and extracts an eligible endpoint inside Rust. The endpoint is used only for:

- a bounded `TcpStream::connect` latency probe;
- an internal country lookup using the existing approved GeoIP mechanism or a country code inferred from the profile label/tag.

Return only normalized ISO alpha-2 country code and bounded latency. Use `None` on parse, lookup, timeout, or unsupported-protocol failure. Sanitize all errors before returning them.

- [ ] **Step 4: Add typed frontend API and metadata cache hook**

Add to `src/lib/types.ts`:

```ts
export interface HomeProfileMetadata {
  country_code: string | null;
  latency_ms: number | null;
}
```

Add to `src/lib/api.ts`:

```ts
getReadyProfileMetadata: (subscriptionId: string, childKey: string) =>
  call<HomeProfileMetadata>("get_ready_profile_metadata", {
    input: { subscription_id: subscriptionId, child_key: childKey },
  }),
```

Implement `useReadyProfileMetadata(profiles)` to:

- query only `ready_config` profiles whose `engine === "xray"`;
- cache by `${subscriptionId}:${key}`;
- avoid duplicate in-flight requests;
- preserve a successful cached result when a later probe fails;
- remove entries for profiles no longer present;
- return an immutable `Map<string, HomeProfileMetadata>`.

- [ ] **Step 5: Run backend and frontend focused tests**

```powershell
cargo test --manifest-path src-tauri\Cargo.toml xray::presentation --lib
npm test -- --run src/hooks/useReadyProfileMetadata.test.ts
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add src-tauri\src\xray\presentation.rs src-tauri\src\commands.rs src-tauri\src\lib.rs src/lib/types.ts src/lib/api.ts src/hooks/useReadyProfileMetadata.ts src/hooks/useReadyProfileMetadata.test.ts
 git commit -m 'feat: expose safe Xray Home metadata'
```

---

## Task 6: Render engine-neutral Home presentation and green connection state

**Files:**
- Modify: `src/components/HomeTab.tsx:28-72, 75-83, 102-132, 150-201, 350-389`
- Modify: `src/App.tsx:809-841, 917-947`
- Test: frontend Home/helper tests

**Interfaces:**
- Consumes: existing sing-box manual latency/GeoIP data plus `Map<string, HomeProfileMetadata>` for Xray ready profiles.
- Produces: same Home layout and labels for both engines, with honest fallbacks.

- [ ] **Step 1: Add failing frontend tests for display mapping and button state**

Test pure helper behavior:

```ts
it("uses safe Xray metadata for a ready profile", () => {
  expect(connectionProfileDisplay(readyXrayProfile, metadata, new Map())).toMatchObject({
    code: "DE",
    ms: 47,
  });
});

it("uses honest fallback when Xray metadata is unavailable", () => {
  expect(connectionProfileDisplay(readyXrayProfile, new Map(), new Map())).toMatchObject({
    code: "??",
    ms: undefined,
  });
});

it("uses green connected classes only while running", () => {
  expect(powerButtonClasses("running")).toContain("bg-success");
  expect(powerButtonClasses("starting")).not.toContain("bg-success");
});
```

Use the project’s actual semantic token names after checking the existing Tailwind/theme definitions; the test must assert the selected semantic connected-state token, not a raw RGB value.

- [ ] **Step 2: Run the focused frontend tests and confirm failure**

```powershell
npm test -- --run src/components/HomeTab.test.tsx
```

Expected: FAIL because the display helper has no Xray metadata input and the connected button is not green.

- [ ] **Step 3: Add metadata to Home props and presentation mapping**

Pass `readyProfileMetadata` from `App.tsx` into `HomeTab`. Update `connectionProfileDisplay` and `ServerPicker` inputs:

- manual profiles: preserve current `flagForProfile` and `latencyByTag` logic;
- Xray ready profiles: look up `${subscriptionId}:${key}` in the safe metadata map;
- subscription link summaries without resolved metadata: keep existing globe/no-latency fallback;
- never render endpoint details or raw engine diagnostics.

For the running hero, use `status.engine` and `status.profile_name` for Xray. Keep sing-box’s `activeOutbound` selector behavior unchanged.

- [ ] **Step 4: Apply semantic green connected-state styling**

Extract a small pure `powerButtonClasses(statusLabel)` helper or equivalent. Use the existing semantic success token for background, border, icon, hover, focus ring, and shadow only when `statusLabel === "running"`. Keep starting/stopping neutral and disabled.

- [ ] **Step 5: Run frontend tests and production build**

```powershell
npm test
npm run build
```

Expected: PASS; Vite may retain the existing large-chunk warning but must produce a successful production build.

- [ ] **Step 6: Commit**

```powershell
git add src/components/HomeTab.tsx src/App.tsx src/hooks/useReadyProfileMetadata.ts src/lib/types.ts src/lib/api.ts src/components/HomeTab.test.tsx
git commit -m 'feat: complete Xray Home presentation parity'
```

---

## Task 7: Verify regression safety and real Xray behavior

**Files:**
- Modify only if a verified in-scope defect is found: relevant files from Tasks 1–6.
- Test: existing Rust/frontend suites and Windows smoke-test artifacts outside git.

- [ ] **Step 1: Run complete static and unit verification**

```powershell
$env:PATH = 'C:\Users\Public\cwdev\rustup-home\toolchains\stable-x86_64-pc-windows-msvc\bin;' + $env:PATH
$env:CLOAKWIRE_TEST_MANIFEST = '1'
cargo fmt --manifest-path src-tauri\Cargo.toml -- --check
cargo check --manifest-path src-tauri\Cargo.toml --lib
cargo test --manifest-path src-tauri\Cargo.toml --lib
npm test
npm run build
git diff --check
```

Acceptance: all Rust and frontend tests pass; no new warnings are treated as failures; no generated or secret files are staged.

- [ ] **Step 2: Validate the generated Xray config with the pinned sidecar**

Use the existing Xray validation path and exact arguments:

```text
run -test -config <path>
```

Confirm that a real prepared config contains:

- loopback-only `api.listen`;
- `StatsService` exactly once;
- `stats` and inbound policy flags;
- the actual traffic-bearing inbound tag in the two queried counter names;
- unchanged provider outbound credentials/routing semantics.

- [ ] **Step 3: Run a real Windows Xray smoke test**

With a valid user subscription and no credential-bearing logs or fixtures:

1. Start an Xray ready profile.
2. Confirm the connection reaches `running` and the Rust-selected dynamic system-proxy endpoint remains active.
3. Generate download and upload traffic through the managed HTTP proxy.
4. Confirm Home receives non-zero `traffic` samples and cumulative totals increase.
5. Confirm the selected Xray profile shows a flag and latency when resolution succeeds; otherwise confirm the globe/em-dash fallback is shown without an error leak.
6. Stop the connection and confirm both the Xray process and telemetry task terminate.
7. Start sing-box and confirm its existing traffic stream, selector/active-server display, latency, flag, system proxy, and updater remain unchanged.
8. Switch engines and confirm no stale Xray samples appear after sing-box starts.

- [ ] **Step 4: Inspect final diff and status**

```powershell
git status --short
git diff --stat origin/main...HEAD
git diff --check origin/main...HEAD
```

Confirm that only the approved Home telemetry feature commits are included in the implementation branch; retain unrelated existing Xray/geodata work for its separate PR integration process.

- [ ] **Step 5: Commit any verified final regression fix separately**

Use a focused commit message naming the concrete defect, for example:

```powershell
git add <only-verified-files>
git commit -m 'fix: <specific Home telemetry regression>'
```

Do not bundle unrelated cleanup, formatting churn, generated artifacts, or Xray updater work.

---

## Xray updater follow-up boundary

Do not implement or expose an Xray update row during Tasks 1–7. The separate updater project must first define and test:

- official release discovery and exact platform asset allowlists;
- redirect, archive, executable, size, SHA-256/signature, and version validation;
- user-data runtime storage, atomic replacement, rollback, and retention of the last verified binary;
- how the running engine is stopped/restarted around installation;
- a sanitized `check_xray_update` / `apply_xray_update` API before the Updates card changes.

This boundary keeps the current Home parity work independently testable and prevents an unverified binary download path from entering the release branch.
