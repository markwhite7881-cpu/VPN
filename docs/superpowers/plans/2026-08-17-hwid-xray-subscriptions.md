# Cloakwire HWID and Xray Fallback Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Windows-first HWID-aware full-configuration subscriptions while keeping sing-box as the automatic primary engine and using Xray-core only for configurations that sing-box cannot execute losslessly.

**Architecture:** Move subscription networking, secret-bearing URLs, full JSON bundles, stable HWID storage, classification, validation, and transactional replacement into focused Rust modules under `src-tauri/src/subscriptions/`. Extend the existing single-process `ProcessManager` with an engine-aware launch specification so only one core can run, then add Xray-specific binary discovery, config adaptation, routing translation, and system-proxy startup behind that boundary. Keep the UI profile-centered: legacy/manual/link-list profiles continue through the existing sing-box generator, while ready-config child profiles carry an automatic engine decision and expose only safe summaries to React.

**Tech Stack:** Rust 2021, Tauri 2, Tokio, reqwest 0.12/rustls, serde/serde_json, uuid, sha2, React 18, TypeScript 5.6, Tailwind/shadcn-style theme tokens, Playwright 1.61, PowerShell release scripts.

## Global Constraints

- Initial implementation and acceptance are Windows-only; Android, macOS, and Linux runtime support are out of scope until explicit Windows approval.
- sing-box remains the default engine for every existing supported share link, link-list subscription, generated config, manual profile, Routing 2.0 feature, TUN mode, system-proxy mode, log view, and updater path.
- Xray is selected automatically only when the original configuration cannot be executed through sing-box without semantic loss; users never choose a core manually.
- Arbitrary Xray configs containing Xray routing, balancers, Observatory, or other Xray-only semantics are not converted to sing-box in the first release.
- The stable HWID is a random UUID persisted in app data; never derive it from a MAC address, hardware serial, Windows product ID, username, or machine name.
- Full-config requests must send `User-Agent: Cloakwire/<version> (<platform>)`, `X-HWID`, `X-Device-OS: Windows`, `X-Device-Model: Cloakwire Desktop`, and `Accept: application/json, text/plain`.
- Require HTTPS for non-localhost full-config subscriptions, reject HTTPS-to-HTTP redirect downgrade, use a 30-second timeout, and cap response bodies at 10 MiB.
- Never log or persist in browser localStorage full subscription URLs, tokens, HWIDs, UUIDs, server addresses, credentials, or complete provider response bodies.
- Existing entries under `singbox-client.subscriptions.v1` remain backward compatible and migrate only after a successful backend import; failed migration leaves the old localStorage data untouched.
- Refresh is transactional: the previous valid bundle survives every fetch, parse, classification, validation, and write failure.
- One response must classify as `link_list`, `singbox_bundle`, or `xray_bundle`; mixed or ambiguous JSON arrays are rejected.
- Xray first release uses Windows system proxy only; do not add WFP, tun2socks, or Xray TUN work.
- For Xray runtime routing, prepend only exact Routing 2.0 equivalents, preserve provider rules in order, preserve provider balancers/Observatory/tags/fallback, and visibly mark unsupported rules.
- `Apps direct` and `Apps via VPN` remain saved and fully functional for sing-box; they are unavailable only while an Xray profile is active.
- Pin the official Xray-core Windows version and SHA-256 in repository-controlled metadata; never execute an unverified downloaded binary and do not implement Xray auto-update in this feature.
- Do not commit binaries, secrets, tokens, generated bundles, app-data fixtures containing credentials, `target/`, `node_modules/`, or release staging.

---

## File Structure

### Rust subscription boundary

- Create `src-tauri/src/subscriptions/mod.rs` — public service API and module exports.
- Create `src-tauri/src/subscriptions/model.rs` — persisted records, safe summaries, response DTOs, engine/kind/error enums.
- Create `src-tauri/src/subscriptions/hwid.rs` — stable random UUID generation, persistence, retrieval, and reset.
- Create `src-tauri/src/subscriptions/metadata.rs` — allowlisted provider-header parsing and `Subscription-Userinfo` parsing.
- Create `src-tauri/src/subscriptions/classify.rs` — link-list/sing-box/Xray/ambiguous payload classification and stable child keys.
- Create `src-tauri/src/subscriptions/http.rs` — secure request client, redirect policy, timeout, size cap, and redacted errors.
- Create `src-tauri/src/subscriptions/store.rs` — app-data paths, atomic JSON writes, migration, and last-valid rollback.
- Create `src-tauri/src/subscriptions/service.rs` — add/list/remove/select/refresh orchestration and engine validation.
- Create `src-tauri/src/subscriptions/tests.rs` — local mock-server integration tests and transaction tests.

### Rust engine boundary

- Create `src-tauri/src/engine/mod.rs` — `EngineKind`, `LaunchSpec`, engine-aware status, validation and binary interfaces.
- Create `src-tauri/src/engine/singbox.rs` — existing sing-box locate/version/check argument construction.
- Create `src-tauri/src/engine/xray.rs` — pinned Xray locate/version/check/run behavior and digest verification.
- Create `src-tauri/src/xray/mod.rs` — Xray runtime config preparation entry point.
- Create `src-tauri/src/xray/inbound.rs` — safe loopback HTTP inbound selection or managed inbound injection.
- Create `src-tauri/src/xray/routing.rs` — exact Routing 2.0 translation, applicability reports, provider-order preservation.
- Modify `src-tauri/src/process.rs` — run one `LaunchSpec`, report active engine/profile, preserve sing-box traffic/TUN behavior, and clear proxy on every exit.
- Modify `src-tauri/src/commands.rs` — thin Tauri wrappers for subscription and unified runtime commands.
- Modify `src-tauri/src/lib.rs` — register modules, managed services, startup cleanup, and commands.
- Modify `src-tauri/src/error.rs` — typed subscription, validation, engine-unavailable, and unsafe-config errors.

### Packaging and verification

- Create `scripts/xray-core.json` — pinned Windows Xray version, official archive URL, archive member, and SHA-256.
- Create `scripts/prepare-xray-sidecar.ps1` — download to temporary staging, verify SHA-256, extract only the expected executable, and copy to the ignored Tauri binary path.
- Create `scripts/test-xray-sidecar.ps1` — validate metadata shape, digest, binary version, and Tauri sidecar name.
- Modify `src-tauri/tauri.conf.json` — add `binaries/xray` beside `binaries/sing-box` for Windows packaging.
- Modify `.gitignore` — ensure generated Xray executables and temporary archives cannot be committed.
- Modify `scripts/release.ps1` — require the verified Xray sidecar before Windows packaging.

### Frontend domain and UI

- Modify `src/lib/types.ts` — safe subscription summaries, child profiles, engine-aware status, active connection target, routing applicability.
- Modify `src/lib/api.ts` — backend subscription CRUD/refresh/HWID and unified engine lifecycle wrappers.
- Rewrite `src/hooks/useSubscriptions.ts` — backend-authoritative state, one-time legacy migration, refresh scheduling, stable child selection.
- Create `src/lib/connectionProfiles.ts` — deterministic merge of manual/link-list profiles and ready-config children into UI choices.
- Modify `src/App.tsx` — engine-automatic connect flow and active target selection without regressing the existing sing-box path.
- Modify `src/components/SubscriptionsCard.tsx` — safe URL display, provider metadata, child profiles, engine badges, HWID controls, typed errors.
- Modify `src/components/ServersTab.tsx` — render ready-config child choices without pretending they are parsed `Outbound` objects.
- Modify `src/components/HomeTab.tsx` and `src/components/ProfileCard.tsx` — show automatic engine badge and selected ready-config profile.
- Modify `src/components/routing/RoutingTab.tsx` — engine capability banner and per-rule applicability while retaining saved process rules.
- Create `tests/subscriptions.spec.ts` — Playwright coverage for migration, child selection, badges, metadata, warnings, and error states.

---

### Task 1: Subscription Domain, Stable HWID, and Atomic Store

**Files:**
- Create: `src-tauri/src/subscriptions/mod.rs`
- Create: `src-tauri/src/subscriptions/model.rs`
- Create: `src-tauri/src/subscriptions/hwid.rs`
- Create: `src-tauri/src/subscriptions/store.rs`
- Modify: `src-tauri/src/lib.rs:18-29`
- Modify: `src-tauri/src/error.rs:11-87`
- Test: inline `#[cfg(test)]` modules in the new files

**Interfaces:**
- Produces: `SubscriptionKind`, `EngineKind`, `SubscriptionErrorKind`, `SubscriptionRecord`, `SubscriptionSummary`, `ChildProfileSummary`, `SubscriptionStore`, `HwidStore`.
- `HwidStore::get_or_create(&self) -> AppResult<Uuid>` returns the same random v4 UUID until reset.
- `HwidStore::reset(&self) -> AppResult<Uuid>` atomically replaces the value with a new v4 UUID.
- `SubscriptionStore::load_all(&self) -> AppResult<Vec<SubscriptionRecord>>` treats a missing store as empty.
- `SubscriptionStore::replace_all(&self, records: &[SubscriptionRecord]) -> AppResult<()>` writes `<file>.tmp`, flushes, and renames over the target.

- [ ] **Step 1: Write failing model and migration tests**

```rust
#[test]
fn old_record_without_kind_defaults_to_auto() {
    let record: SubscriptionRecord = serde_json::from_value(json!({
        "id": "legacy-1",
        "name": "Legacy",
        "url": "https://example.test/sub",
        "interval_minutes": 60
    })).unwrap();
    assert_eq!(record.kind, SubscriptionKind::Auto);
    assert_eq!(record.engine, None);
}

#[test]
fn summary_never_serializes_secret_url_or_bundle() {
    let value = serde_json::to_value(sample_record().to_summary()).unwrap();
    assert!(value.get("url").is_none());
    assert!(value.get("bundle").is_none());
}
```

- [ ] **Step 2: Run the focused tests and verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml subscriptions::model -- --nocapture`

Expected: FAIL because the `subscriptions` module and domain types do not exist.

- [ ] **Step 3: Implement explicit persisted and safe DTO types**

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionKind { Auto, LinkList, SingboxBundle, XrayBundle }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EngineKind { Singbox, Xray }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionRecord {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub kind: SubscriptionKind,
    #[serde(default)]
    pub engine: Option<EngineKind>,
    pub interval_minutes: u32,
    #[serde(default)]
    pub active_child_key: Option<String>,
    #[serde(default)]
    pub children: Vec<ChildProfileRecord>,
    #[serde(default)]
    pub metadata: ProviderMetadata,
    #[serde(default)]
    pub last_success_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_http_status: Option<u16>,
    #[serde(default)]
    pub last_error: Option<SubscriptionFailure>,
}
```

Implement `Default` for additive fields and a `to_summary()` conversion that excludes `url`, raw configs, parsed credentials, and full error bodies.

- [ ] **Step 4: Write failing HWID persistence tests**

```rust
#[test]
fn get_or_create_is_stable_and_reset_rotates() {
    let dir = tempfile::tempdir().unwrap();
    let store = HwidStore::new(dir.path().join("device-id"));
    let first = store.get_or_create().unwrap();
    let second = store.get_or_create().unwrap();
    let third = store.reset().unwrap();
    assert_eq!(first, second);
    assert_ne!(first, third);
    assert_eq!(third.get_version_num(), 4);
}
```

- [ ] **Step 5: Add test-only filesystem dependency and implement atomic helpers**

Add under `[dev-dependencies]` in `src-tauri/Cargo.toml`:

```toml
tempfile = "3"
```

Use `OpenOptions::create_new(true)` for the temporary HWID file, `sync_all()`, and rename. Reject non-UUID or non-v4 contents instead of silently treating hardware-derived text as valid.

- [ ] **Step 6: Add typed errors and serialization tests**

Add variants whose serialized kinds are exact and frontend-stable:

```rust
Subscription(String),       // kind: subscription
SubscriptionAuth(String),   // kind: subscription_auth
SubscriptionExpired(String),// kind: subscription_expired
DeviceLimit(String),        // kind: device_limit
PayloadTooLarge,            // kind: payload_too_large
UnsafeRedirect(String),     // kind: unsafe_redirect
AmbiguousConfig(String),    // kind: ambiguous_config
Validation(String),         // kind: validation
EngineUnavailable(String),  // kind: engine_unavailable
UnsafeConfig(String),       // kind: unsafe_config
```

- [ ] **Step 7: Run Rust tests and formatting**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`

Run: `cargo test --manifest-path src-tauri/Cargo.toml subscriptions:: -- --nocapture`

Expected: PASS; summaries contain no URL/bundle fields, atomic replacement survives reload, and HWID reset rotates only after a successful write.

- [ ] **Step 8: Commit**

```powershell
git add src-tauri/Cargo.toml src-tauri/src/error.rs src-tauri/src/lib.rs src-tauri/src/subscriptions
git commit -m "feat: add secure subscription storage domain"
```

---

### Task 2: Secure HTTP Client, Metadata Parsing, and Payload Classification

**Files:**
- Create: `src-tauri/src/subscriptions/http.rs`
- Create: `src-tauri/src/subscriptions/metadata.rs`
- Create: `src-tauri/src/subscriptions/classify.rs`
- Modify: `src-tauri/src/subscriptions/mod.rs`
- Test: inline unit tests plus `src-tauri/src/subscriptions/tests.rs`

**Interfaces:**
- Consumes: `HwidStore`, `SubscriptionKind`, `EngineKind`, `ProviderMetadata`, typed `AppError` variants from Task 1.
- Produces: `SubscriptionHttpClient::fetch(&Url, Uuid, &str, &str) -> AppResult<FetchedPayload>`.
- Produces: `classify_payload(bytes: &[u8], content_type: Option<&str>) -> AppResult<ClassifiedPayload>`.
- `ClassifiedPayload` is `LinkList(ParseLinksResult)`, `SingboxBundle(Vec<ClassifiedChild>)`, or `XrayBundle(Vec<ClassifiedChild>)`.
- Produces: `stable_child_key(value: &Value, index: usize, duplicate_ordinal: usize) -> String` based only on non-secret provider identity.

- [ ] **Step 1: Write classifier tests for all accepted and rejected shapes**

```rust
#[test]
fn classifies_xray_array_from_protocol_and_routing_markers() {
    let payload = br#"[{"remarks":"Auto","outbounds":[{"protocol":"vless"}],"routing":{"balancers":[]},"observatory":{}}]"#;
    assert!(matches!(classify_payload(payload, Some("application/json")).unwrap(), ClassifiedPayload::XrayBundle(_)));
}

#[test]
fn rejects_mixed_engine_array() {
    let payload = br#"[{"outbounds":[{"type":"direct"}]},{"outbounds":[{"protocol":"freedom"}]}]"#;
    assert!(matches!(classify_payload(payload, Some("application/json")), Err(AppError::AmbiguousConfig(_))));
}
```

Also cover plain links, base64 links, sing-box `outbounds[].type`, malformed JSON, empty arrays, scalar JSON, and JSON with no engine markers.

- [ ] **Step 2: Run focused classifier tests and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml subscriptions::classify -- --nocapture`

Expected: FAIL because classification functions do not exist.

- [ ] **Step 3: Implement content-first classification**

Parse JSON only when the body begins with `{` or `[` after whitespace or declares JSON. Require every object in an array to resolve to the same engine. Prefer explicit marker sets over provider names:

```rust
let singbox = has_outbound_key(value, "type") || value.get("route").is_some();
let xray = has_outbound_key(value, "protocol")
    || pointer_exists(value, "/routing/domainStrategy")
    || pointer_exists(value, "/routing/balancers")
    || value.get("observatory").is_some();
```

Reject objects where both marker sets are true unless an explicit future adapter is added and tested.

- [ ] **Step 4: Write metadata parser tests**

```rust
#[test]
fn parses_allowlisted_headers_and_userinfo() {
    let metadata = parse_metadata(&headers(&[
        ("Profile-Title", "Cloakwire Demo"),
        ("Profile-Update-Interval", "6"),
        ("Subscription-Userinfo", "upload=1; download=2; total=100; expire=2000000000"),
        ("Set-Cookie", "secret=never-copy")
    ])).unwrap();
    assert_eq!(metadata.profile_title.as_deref(), Some("Cloakwire Demo"));
    assert_eq!(metadata.update_interval_hours, Some(6));
    assert_eq!(metadata.userinfo.unwrap().total, Some(100));
}
```

- [ ] **Step 5: Implement strict allowlisted metadata parsing**

Accept only `Profile-Title`, `Profile-Update-Interval`, `Profile-Web-Page-Url`, `Support-Url`, and `Subscription-Userinfo`. Parse only HTTP(S) support/web URLs, clamp update intervals to `15 minutes..=30 days`, and never retain unknown headers.

- [ ] **Step 6: Write local-server tests for headers, timeout, body cap, and redirects**

Use `tokio::net::TcpListener` fixtures in `subscriptions/tests.rs`. Assert the request contains the exact User-Agent and device headers without printing them. Cover:

```rust
assert_eq!(captured.header("accept"), "application/json, text/plain");
assert_eq!(captured.header("x-device-os"), "Windows");
assert_eq!(captured.header("x-device-model"), "Cloakwire Desktop");
assert!(captured.header("x-hwid").parse::<Uuid>().is_ok());
```

- [ ] **Step 7: Implement the secure reqwest client**

Build a client with `redirect::Policy::custom`, `timeout(Duration::from_secs(30))`, and streaming reads via `bytes_stream()`. Reject non-local HTTP before sending. Track cumulative bytes and stop above `10 * 1024 * 1024`. On redirects, compare previous and next schemes and reject `https -> http` unless both hosts are loopback/localhost.

- [ ] **Step 8: Verify no sensitive error interpolation**

Search the new modules for URL/HWID/body formatting and replace diagnostics with host-free categories and HTTP status only.

Run: `cargo test --manifest-path src-tauri/Cargo.toml subscriptions:: -- --nocapture`

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`

Expected: PASS with no request URL, HWID, response body, token, UUID, or server address emitted by test diagnostics.

- [ ] **Step 9: Commit**

```powershell
git add src-tauri/src/subscriptions
git commit -m "feat: securely classify full config subscriptions"
```

---

### Task 3: Transactional Subscription Service and Tauri Commands

**Files:**
- Create: `src-tauri/src/subscriptions/service.rs`
- Modify: `src-tauri/src/subscriptions/mod.rs`
- Modify: `src-tauri/src/commands.rs:585-621`
- Modify: `src-tauri/src/lib.rs:48-131`
- Test: `src-tauri/src/subscriptions/tests.rs`

**Interfaces:**
- Consumes: store, HTTP client, metadata, classifier from Tasks 1-2.
- Produces: `SubscriptionService::list`, `add`, `remove`, `set_interval`, `select_child`, `refresh`, `migrate_legacy`, `get_hwid`, and `reset_hwid`.
- Produces Tauri commands: `list_subscriptions`, `add_subscription`, `remove_subscription`, `set_subscription_interval`, `select_subscription_child`, `refresh_subscription`, `migrate_legacy_subscriptions`, `get_subscription_hwid`, `reset_subscription_hwid`.
- Returns `SubscriptionSnapshot { subscriptions: Vec<SubscriptionSummary>, link_outbounds: Vec<SubscriptionOutbounds> }` and never returns raw ready configs or stored URLs.

- [ ] **Step 1: Write rollback and stable-selection tests**

```rust
#[tokio::test]
async fn failed_refresh_keeps_last_valid_bundle_and_selection() {
    let service = fixture_with_valid_xray_bundle().await;
    service.server.reply_next(200, br#"[{"not":"classifiable"}]"#).await;
    let before = service.store.load("sub-1").unwrap();
    assert!(service.refresh("sub-1").await.is_err());
    let after = service.store.load("sub-1").unwrap();
    assert_eq!(after.active_child_key, before.active_child_key);
    assert_eq!(after.bundle_digest, before.bundle_digest);
}
```

Also cover credential changes retaining a stable child key, reordered children, removed selected child selecting the first valid child with `selection_changed=true`, and partial-array validation rejecting the entire refresh.

- [ ] **Step 2: Run service tests and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml subscriptions::tests -- --nocapture`

Expected: FAIL because the orchestration service does not exist.

- [ ] **Step 3: Implement add/list/remove/update operations under one service lock**

Use `tokio::sync::Mutex<()>` around read-modify-write operations. `add` generates the subscription ID in Rust, stores the secret URL only in the backend record, performs the first refresh, and commits only if refresh succeeds. `remove` deletes the record and its bundle atomically.

- [ ] **Step 4: Implement refresh as prepare-then-commit**

```rust
pub async fn refresh(&self, id: &str) -> AppResult<RefreshSubscriptionResult> {
    let current = self.store.load(id)?;
    let fetched = self.http.fetch(&Url::parse(&current.url)?, self.hwid.get_or_create()?, &self.version, "Windows").await?;
    let candidate = self.prepare_candidate(&current, fetched).await?;
    self.validate_every_child(&candidate).await?;
    self.store.replace(id, &candidate)?;
    Ok(candidate.to_safe_result())
}
```

The candidate must remain in memory or a temporary file until all children validate. Store a SHA-256 digest of each raw child config for change detection, but do not expose it as a user identifier.

- [ ] **Step 5: Implement legacy localStorage migration input**

```rust
#[derive(Deserialize)]
pub struct LegacySubscriptionInput {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(rename = "intervalMinutes")]
    pub interval_minutes: u32,
}
```

Deduplicate by exact backend ID first, then by normalized URL digest internally. Do not include the URL in the migration result.

- [ ] **Step 6: Replace the old `fetch_subscription` implementation with compatibility delegation**

Keep `fetch_subscription(url)` temporarily for old frontend builds, but route it through the secure client with a transient ID and return only `ParseLinksResult` when classification is `link_list`. Return `Unsupported` for full-config bundles so old UI cannot accidentally flatten them.

- [ ] **Step 7: Register managed state and commands**

Create app-data paths in `lib.rs` setup using `app.path().app_data_dir()`. Manage `Arc<SubscriptionService>` before invoking commands. Register all new commands without removing existing parser/config commands.

- [ ] **Step 8: Run service and full Rust tests**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`

Run: `cargo test --manifest-path src-tauri/Cargo.toml --all-targets -- --nocapture`

Expected: PASS, including legacy link-list behavior and failed-refresh rollback.

- [ ] **Step 9: Commit**

```powershell
git add src-tauri/src/commands.rs src-tauri/src/lib.rs src-tauri/src/subscriptions
git commit -m "feat: add transactional subscription service"
```

---

### Task 4: Backend-Authoritative Frontend Subscription State

**Files:**
- Modify: `src/lib/types.ts:1-378`
- Modify: `src/lib/api.ts:7-140`
- Rewrite: `src/hooks/useSubscriptions.ts`
- Create: `src/lib/connectionProfiles.ts`
- Modify: `src/components/SubscriptionsCard.tsx`
- Modify: `src/components/ServersTab.tsx`
- Test: `tests/subscriptions.spec.ts`
- Modify: `package.json`

**Interfaces:**
- Consumes: safe command DTOs from Task 3.
- Produces: `SubscriptionSummary`, `SubscriptionChildProfile`, `ConnectionProfile`, `SubscriptionSnapshot`, `RefreshSubscriptionResult` TypeScript mirrors.
- Produces hook methods `add`, `remove`, `refreshOne`, `refreshAll`, `setIntervalFor`, `selectChild`, `getHwid`, and `resetHwid`.
- `ConnectionProfile` is a discriminated union and never coerces ready configs into `Outbound`.

- [ ] **Step 1: Add a frontend test command and failing migration test**

Add Vitest explicitly rather than assuming it exists:

```json
"scripts": {
  "test": "vitest run",
  "test:ui": "playwright test"
},
"devDependencies": {
  "vitest": "^2.1.9"
}
```

Create `src/lib/connectionProfiles.test.ts` and assert legacy localStorage is removed only after `migrateLegacySubscriptions` resolves successfully.

- [ ] **Step 2: Run the focused frontend test and verify failure**

Run: `npm install`

Run: `npm test -- src/lib/connectionProfiles.test.ts`

Expected: FAIL because the new API and migration helper do not exist.

- [ ] **Step 3: Add exact TypeScript DTOs**

```ts
export type EngineKind = "singbox" | "xray";
export type SubscriptionKind = "auto" | "link_list" | "singbox_bundle" | "xray_bundle";

export interface SubscriptionChildProfile {
  key: string;
  name: string;
  engine: EngineKind;
  selected: boolean;
  validation: "valid" | "unavailable";
}

export type ConnectionProfile =
  | { kind: "outbound"; key: string; engine: "singbox"; outbound: Outbound; source: "manual" | "subscription" }
  | { kind: "ready_config"; key: string; engine: EngineKind; subscriptionId: string; childKey: string; name: string };
```

Extend `StatusReport` with `engine: EngineKind | null`, `profile_key: string | null`, and `profile_name: string | null` using nullable defaults so older backend mocks remain readable.

- [ ] **Step 4: Implement typed API wrappers**

```ts
listSubscriptions: () => call<SubscriptionSnapshot>("list_subscriptions"),
addSubscription: (input: AddSubscriptionInput) => call<RefreshSubscriptionResult>("add_subscription", { input }),
refreshSubscription: (id: string) => call<RefreshSubscriptionResult>("refresh_subscription", { id }),
selectSubscriptionChild: (id: string, childKey: string) => call<SubscriptionSummary>("select_subscription_child", { id, childKey }),
getSubscriptionHwid: () => call<string>("get_subscription_hwid"),
resetSubscriptionHwid: () => call<string>("reset_subscription_hwid"),
```

- [ ] **Step 5: Rewrite the hook around backend snapshots**

On mount:

1. Read `singbox-client.subscriptions.v1` once.
2. If valid legacy entries exist, call `migrateLegacySubscriptions(entries)`.
3. Remove the key only after successful migration.
4. Call `listSubscriptions()` and populate summaries plus link-list outbounds.
5. Schedule refresh by backend summary timestamps and intervals.

Never write the new subscription summaries or URLs back to localStorage.

- [ ] **Step 6: Add deterministic connection-profile merging tests**

Assert manual profiles keep their order, link-list profiles follow their owning subscription order, and ready-config children retain stable keys across refresh/reorder. Assert engine selection is copied from the backend and cannot be overridden in UI state.

- [ ] **Step 7: Update subscription and server UI with theme tokens only**

Render provider title, type, safe hostname label returned by Rust, support/web links, traffic/expiry, requested refresh interval, typed error badge, and child rows. Show `sing-box` or `Xray` badges but no core selector. Do not display the full secret URL in `title`, text, DOM attributes, or clipboard.

- [ ] **Step 8: Run frontend tests, typecheck, and production build**

Run: `npm test`

Run: `npm run build`

Expected: PASS; the browser bundle contains no hardcoded provider hostname or test token.

- [ ] **Step 9: Commit**

```powershell
git add package.json package-lock.json src/lib src/hooks/useSubscriptions.ts src/components/SubscriptionsCard.tsx src/components/ServersTab.tsx tests/subscriptions.spec.ts
git commit -m "feat: move subscriptions behind secure backend state"
```

---

### Task 5: Engine-Aware Single Process Runtime Without sing-box Regression

**Files:**
- Create: `src-tauri/src/engine/mod.rs`
- Create: `src-tauri/src/engine/singbox.rs`
- Modify: `src-tauri/src/process.rs:14-520`
- Modify: `src-tauri/src/commands.rs:17-175`
- Modify: `src-tauri/src/lib.rs:18-131`
- Modify: `src/lib/types.ts:6-32`
- Modify: `src/lib/api.ts:46-83`
- Test: `src-tauri/src/process.rs` inline tests

**Interfaces:**
- Consumes: `EngineKind` from the backend domain; re-export it from `engine` so one Rust enum is authoritative.
- Produces: `LaunchSpec { engine, binary, args, config_path, controller_url, profile_key, profile_name }`.
- Produces unified commands `start_connection`, `stop_connection`, `get_status`, and `get_logs` while retaining old sing-box commands as wrappers during migration.
- `StatusReport` gains `engine`, `profile_key`, and `profile_name`.

- [ ] **Step 1: Write failing exclusivity and status tests**

```rust
#[tokio::test]
async fn start_rejects_second_engine_while_first_is_active() {
    let pm = Arc::new(ProcessManager::new());
    pm.install_test_child(EngineKind::Singbox).await;
    let err = pm.start_spec(test_xray_spec()).await.unwrap_err();
    assert!(matches!(err, AppError::AlreadyRunning(_)));
}

#[test]
fn stopped_status_has_no_engine_or_profile() {
    let status = StatusReport::default();
    assert_eq!(status.engine, None);
    assert_eq!(status.profile_key, None);
}
```

- [ ] **Step 2: Run process tests and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml process:: -- --nocapture`

Expected: FAIL because status and launch specifications are sing-box-specific.

- [ ] **Step 3: Extract sing-box discovery and validation into `engine/singbox.rs`**

Move `locate_binary`, version parsing, and `check -c` argument construction without changing search order, updater preference, target-triple naming, TUN checks, traffic stream, or Clash controller behavior.

- [ ] **Step 4: Generalize process launch minimally**

```rust
pub struct LaunchSpec {
    pub engine: EngineKind,
    pub binary: PathBuf,
    pub args: Vec<OsString>,
    pub config_path: PathBuf,
    pub controller_url: Option<String>,
    pub profile_key: Option<String>,
    pub profile_name: Option<String>,
}
```

`ProcessManager::start_spec` uses the provided args. Execute sing-box-only TUN capability/DNS/traffic behavior only when `spec.engine == EngineKind::Singbox`. Keep one child slot, one log ring, and unconditional system-proxy cleanup on exit.

- [ ] **Step 5: Preserve legacy command behavior through wrappers**

`start_singbox_with_config` builds a sing-box `LaunchSpec` and delegates to `start_connection`. `stop_singbox` delegates to `stop_connection`. Existing frontend calls must still pass before Task 8 switches them.

- [ ] **Step 6: Make logs identify only the core, not secret config paths**

Replace `starting sing-box with config <path>` with `starting sing-box profile` or `starting Xray profile`. Config file paths can reveal subscription IDs and must remain debug-only and redacted.

- [ ] **Step 7: Run all Rust and existing frontend builds**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --all-targets`

Run: `npm run build`

Expected: PASS; existing manual/link-list sing-box connection behavior compiles unchanged.

- [ ] **Step 8: Commit**

```powershell
git add src-tauri/src/engine src-tauri/src/process.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src/lib/types.ts src/lib/api.ts
git commit -m "refactor: add engine-aware single process runtime"
```

---

### Task 6: Xray Config Preparation, Inbound Safety, and Routing Translation

**Files:**
- Create: `src-tauri/src/xray/mod.rs`
- Create: `src-tauri/src/xray/inbound.rs`
- Create: `src-tauri/src/xray/routing.rs`
- Modify: `src-tauri/src/config/mod.rs` only to expose shared Routing 2.0 DTOs if required
- Test: inline tests in all Xray modules

**Interfaces:**
- Consumes: provider Xray `serde_json::Value` and current `GeneratorSettings.routing`.
- Produces: `prepare_xray_runtime_config(provider, routing, port_allocator) -> AppResult<PreparedXrayConfig>`.
- `PreparedXrayConfig { value, proxy_host, proxy_port, applicability }`.
- `RoutingApplicability { applied_rule_ids, unavailable: Vec<UnavailableRule> }`.
- `UnavailableRule { rule_id, reason: ProcessMatcher | UnsupportedMatcher | UnsupportedAction | MissingOutboundTag | MissingBalancerTag }`.

- [ ] **Step 1: Write inbound selection/injection tests**

```rust
#[test]
fn selects_unambiguous_loopback_http_inbound() {
    let config = json!({"inbounds":[{"tag":"local-http","listen":"127.0.0.1","port":10809,"protocol":"http"}]});
    let result = ensure_managed_http_inbound(config, || Ok(20809)).unwrap();
    assert_eq!(result.proxy_port, 10809);
    assert!(!result.injected);
}

#[test]
fn injects_runtime_only_http_inbound_when_missing() {
    let result = ensure_managed_http_inbound(json!({"inbounds":[]}), || Ok(20809)).unwrap();
    assert_eq!(result.proxy_host, "127.0.0.1");
    assert_eq!(result.proxy_port, 20809);
    assert_eq!(result.value["inbounds"][0]["tag"], "cloakwire-managed-http");
}
```

Reject wildcard/public HTTP listeners, ambiguous multiple candidates, invalid ports, and tag collision with `cloakwire-managed-http`.

- [ ] **Step 2: Write routing precedence and preservation tests**

```rust
#[test]
fn prepends_exact_local_rules_before_provider_rules() {
    let prepared = merge_routing(provider_with_balancer_and_rules(), local_domain_rule()).unwrap();
    assert_eq!(prepared.value["routing"]["rules"][0]["domain"][0], "full:example.com");
    assert_eq!(prepared.value["routing"]["rules"][1]["balancerTag"], "leastPing");
    assert_eq!(prepared.value["observatory"], provider_with_balancer_and_rules()["observatory"]);
}
```

Assert provider `domainStrategy`, `domainMatcher`, `balancers`, original rule order, outbound tags, and fallback remain byte-equivalent at the JSON value level.

- [ ] **Step 3: Implement exact matcher translation**

Support only:

- `domain` -> `full:<value>`
- `domain_suffix` -> `domain:<value>`
- `domain_keyword` -> `keyword:<value>`
- supported `geosite:*` strings without rewriting
- `ip_cidr` and supported `geoip:*` strings -> `ip`
- destination `port` and `port_range`
- network values `tcp` and `udp`
- protocol strings accepted by Xray
- explicit valid inbound tags

Every local rule with `process_name`, process path/regex, source matchers, sing-box rule-set references, invert, sniff/resolve/hijack actions, or unknown fields returns an unavailable reason and is not silently emitted.

- [ ] **Step 4: Implement exact action translation**

Route actions may target only an existing provider `outboundTag` or `balancerTag`. Reject missing targets. For `reject`, reuse an existing `blackhole` outbound or inject one with reserved tag `cloakwire-block` only when that tag is unused.

- [ ] **Step 5: Add sanitization and mutation-boundary tests**

Assert the stored provider value remains unchanged after runtime preparation. Assert diagnostics include sanitized child display name and rule label only, never UUIDs, servers, or the full JSON.

- [ ] **Step 6: Run focused and full Rust tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml xray:: -- --nocapture`

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`

Expected: PASS with explicit applicability for every enabled local rule.

- [ ] **Step 7: Commit**

```powershell
git add src-tauri/src/xray src-tauri/src/config/mod.rs
git commit -m "feat: prepare safe Xray runtime configurations"
```

---

### Task 7: Verified Xray Sidecar and Runtime Integration

**Files:**
- Create: `src-tauri/src/engine/xray.rs`
- Create: `scripts/xray-core.json`
- Create: `scripts/prepare-xray-sidecar.ps1`
- Create: `scripts/test-xray-sidecar.ps1`
- Modify: `src-tauri/tauri.conf.json:42-44`
- Modify: `.gitignore:7-8`
- Modify: `scripts/release.ps1:35-39`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: inline Rust tests and PowerShell validation script

**Interfaces:**
- Consumes: engine-aware `ProcessManager` and prepared Xray config.
- Produces: `xray::locate_binary`, `xray::probe_version`, `xray::validate_config`, `xray::launch_spec`.
- Produces unified command `start_ready_profile(subscription_id, child_key, routing)` that resolves the stored engine server-side.

- [ ] **Step 1: Write failing Rust tests for Xray discovery and validation arguments**

```rust
#[test]
fn xray_validation_uses_test_config_command() {
    let args = validation_args(Path::new("runtime.json"));
    assert_eq!(args, vec!["run", "-test", "-config", "runtime.json"]);
}

#[test]
fn xray_launch_uses_original_runtime_config_path() {
    let args = run_args(Path::new("runtime.json"));
    assert_eq!(args, vec!["run", "-config", "runtime.json"]);
}
```

- [ ] **Step 2: Pin the independently verified official Xray release**

`scripts/xray-core.json` must contain these exact repository-controlled values:

```json
{
  "version": "v26.3.27",
  "platform": "windows-64",
  "archiveUrl": "https://github.com/XTLS/Xray-core/releases/download/v26.3.27/Xray-windows-64.zip",
  "archiveMember": "xray.exe",
  "archiveSha256": "d004c39288ce9ada487c6f398c7c545f7d749e44bdfdd59dbc9f865afba4e1ad",
  "executableSha256": "15c2d007954ac53ba69b80ec91242786b3c0b71d52649165b4ca1d5cc96ef8f1",
  "executableSize": 35613696
}
```

The values above were independently calculated from the official XTLS `v26.3.27` Windows archive and its exact root member `xray.exe`. The validation script must reject placeholder text, non-HTTPS URLs, non-XTLS GitHub release routes, digest fields that are not 64 lowercase hexadecimal characters, and a non-positive executable size.

- [ ] **Step 3: Implement safe sidecar preparation**

`prepare-xray-sidecar.ps1` must:

1. Parse `xray-core.json`.
2. Download into a fresh directory under the supplied staging root.
3. Compute the archive SHA-256 and compare it with `archiveSha256` using `-ceq` after lowercase normalization.
4. Open the ZIP and extract only exact root member `xray.exe`.
5. Require the extracted byte length to equal `executableSize` and its SHA-256 to equal `executableSha256`.
6. Run `xray.exe version` and require the pinned version.
7. Copy to `src-tauri/binaries/xray-x86_64-pc-windows-msvc.exe`.
8. Remove temporary archive/staging in `finally`.

- [ ] **Step 4: Implement runtime digest defense in Rust**

Compile the expected executable SHA-256 and size into `engine/xray.rs` using `include_str!("../../../scripts/xray-core.json")` only for non-secret metadata. On first discovery per app run, verify the executable byte length and hash, and reject any mismatch before version/check/run.

- [ ] **Step 5: Add the sidecar to Tauri packaging and release gates**

```json
"externalBin": [
  "binaries/sing-box",
  "binaries/xray"
]
```

`release.ps1` calls `scripts/test-xray-sidecar.ps1` before `npm run tauri:build` and refuses to package when the file is missing, has a wrong digest/version, or uses the wrong target-triple filename.

- [ ] **Step 6: Implement server-side ready-profile start**

The command loads the selected child from `SubscriptionService`, never accepts raw config JSON from the WebView, prepares an engine-specific runtime copy in app temp data, validates it with the selected core, stops any current process, starts the new `LaunchSpec`, and applies the returned local system proxy only after status becomes running. On any failure, clear the proxy and leave status stopped.

- [ ] **Step 7: Run validation gates**

Run: `cargo test --manifest-path src-tauri/Cargo.toml engine::xray -- --nocapture`

Run: `& .\scripts\test-xray-sidecar.ps1`

Run: `npm run build`

Expected: PASS; `xray.exe version` matches pinned metadata and the executable digest matches both preparation and runtime expectations.

- [ ] **Step 8: Commit metadata and code, not the executable**

```powershell
git status --short
git add scripts/xray-core.json scripts/prepare-xray-sidecar.ps1 scripts/test-xray-sidecar.ps1 scripts/release.ps1 src-tauri/tauri.conf.json .gitignore src-tauri/src/engine/xray.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat: add verified Windows Xray fallback runtime"
```

Confirm `git status --short` does not stage any `.exe`, `.zip`, or credential-bearing config.

---

### Task 8: Automatic Engine Selection and Unified Connect Flow

**Files:**
- Modify: `src/App.tsx:342-608,780-959`
- Modify: `src/lib/connectionProfiles.ts`
- Modify: `src/lib/api.ts`
- Modify: `src/components/HomeTab.tsx`
- Modify: `src/components/ProfileCard.tsx`
- Modify: `src/components/ServersTab.tsx`
- Test: `src/lib/connectionProfiles.test.ts`
- Test: `tests/subscriptions.spec.ts`

**Interfaces:**
- Consumes: `ConnectionProfile`, backend `start_ready_profile`, and legacy sing-box generation path.
- Produces one `onStart` branch selected by the profile discriminant, not by a user core setting.
- Manual/link-list outbounds call `generateConfig -> saveConfigToPath -> startSingboxWithConfig` exactly as before.
- Ready-config children call `startReadyProfile(subscriptionId, childKey, settings.routing)` and trust the backend engine decision.

- [ ] **Step 1: Write failing engine-selection tests**

```ts
it("keeps link-list profiles on sing-box", () => {
  expect(connectPlan(outboundProfile).engine).toBe("singbox");
  expect(connectPlan(outboundProfile).mode).toBe("generated");
});

it("uses backend-selected Xray only for an Xray child", () => {
  expect(connectPlan(xrayChild).engine).toBe("xray");
  expect(connectPlan(xrayChild).mode).toBe("ready_config");
});
```

Assert no `setEngine`, core dropdown, or manual override exists.

- [ ] **Step 2: Run frontend tests and verify failure**

Run: `npm test -- src/lib/connectionProfiles.test.ts`

Expected: FAIL until `connectPlan` and the unified profile model are implemented.

- [ ] **Step 3: Implement one selected connection target**

Persist only the safe `ConnectionProfile.key` in localStorage. Restore it if present; otherwise select the first valid profile. For ready configs, preserve backend child selection by stable key after refresh.

- [ ] **Step 4: Split `onStart` without changing the existing generated branch**

```ts
if (selectedProfile.kind === "outbound") {
  const value = await api.generateConfig(profiles, settings);
  const path = await api.saveConfigToPath(value);
  const next = await api.startSingboxWithConfig(path, controllerUrl);
  // existing system-proxy branch remains unchanged
} else {
  const next = await api.startReadyProfile(
    selectedProfile.subscriptionId,
    selectedProfile.childKey,
    settings.routing,
  );
}
```

Do not pass raw stored config or engine choice from React.

- [ ] **Step 5: Unify stop/status/log rendering**

Use `stopConnection`, `getStatus`, and `getLogs`. Show `sing-box` or `Xray` from `StatusReport.engine`. Disable Clash proxy-selection/traffic widgets for Xray because no Clash controller is guaranteed.

- [ ] **Step 6: Add Playwright selection and regression scenarios**

Mock Tauri invoke calls and verify:

- adding a legacy link-list still produces sing-box outbound cards;
- selecting an Xray child produces one `start_ready_profile` call;
- no `generate_config` call occurs for Xray;
- returning to a sing-box profile restores process-routing controls and existing settings;
- removing/reordering provider children retains selection by child key.

- [ ] **Step 7: Run frontend and Rust regression gates**

Run: `npm test`

Run: `npm run test:ui`

Run: `npm run build`

Run: `cargo test --manifest-path src-tauri/Cargo.toml --all-targets`

Expected: PASS with the old sing-box branch covered and unchanged in behavior.

- [ ] **Step 8: Commit**

```powershell
git add src/App.tsx src/lib/api.ts src/lib/connectionProfiles.ts src/lib/connectionProfiles.test.ts src/components/HomeTab.tsx src/components/ProfileCard.tsx src/components/ServersTab.tsx tests/subscriptions.spec.ts
git commit -m "feat: select VPN engine automatically per profile"
```

---

### Task 9: Routing Capability Reporting, Provider Metadata, and HWID UX

**Files:**
- Modify: `src/components/routing/RoutingTab.tsx`
- Modify: `src/components/SubscriptionsCard.tsx`
- Modify: `src/App.tsx`
- Modify: `src/lib/types.ts`
- Test: `tests/subscriptions.spec.ts`

**Interfaces:**
- Consumes: active engine and `RoutingApplicability` from backend start/preview results.
- Produces: visible engine capability notice and per-rule status without mutating saved settings.
- HWID display is user-triggered; reset requires an explicit confirmation warning.

- [ ] **Step 1: Write failing UI tests for Xray limitations**

```ts
await expect(page.getByText("Apps via VPN is unavailable for this Xray profile")).toBeVisible();
await expect(page.getByText("Your saved app rules will resume with sing-box")).toBeVisible();
await expect(page.getByRole("button", { name: "Reset device ID" })).toBeVisible();
```

Also assert the process names remain present after switching Xray -> sing-box.

- [ ] **Step 2: Run Playwright and verify failure**

Run: `npm run test:ui -- tests/subscriptions.spec.ts`

Expected: FAIL because capability and HWID controls are not rendered.

- [ ] **Step 3: Add engine-aware routing capability props**

Pass `activeEngine`, `selectedProfileEngine`, and `routingApplicability` into `RoutingTab`. When selected engine is Xray:

- keep all controls and values mounted;
- disable process pickers only;
- show that process rules are saved but not applied;
- mark exact translated custom rules as applied;
- mark unsupported rules with the backend reason;
- do not claim TUN mode can enable Xray process matching.

For sing-box, retain the current TUN-mode warning and behavior exactly.

- [ ] **Step 4: Add provider metadata and child capability UI**

Use existing `bg-card`, `text-foreground`, `text-muted-foreground`, `border-border`, and semantic destructive/warning tokens. Show profile title, type, expiry, traffic, safe web/support links, requested refresh interval, and engine badge. Keep the UI profile-centered and avoid introducing a global core settings section.

- [ ] **Step 5: Add HWID copy and guarded reset**

Fetch HWID only when the user expands device information. Confirmation text must state: `Resetting the device ID may make the provider count this computer as a new device.` After successful reset, refresh displayed value but do not automatically refresh all subscriptions.

- [ ] **Step 6: Run UI and production build gates**

Run: `npm run test:ui`

Run: `npm test`

Run: `npm run build`

Expected: PASS; no full subscription URL or credential appears in DOM snapshots.

- [ ] **Step 7: Commit**

```powershell
git add src/App.tsx src/lib/types.ts src/components/SubscriptionsCard.tsx src/components/routing/RoutingTab.tsx tests/subscriptions.spec.ts
git commit -m "feat: report Xray routing capabilities safely"
```

---

### Task 10: End-to-End Windows Validation and Delivery Build

**Files:**
- Create: `scripts/test-hwid-xray-subscriptions.ps1`
- Modify: `scripts/test-validate-release.ps1` if a sidecar inventory assertion belongs there
- Modify: `docs/superpowers/specs/2026-08-17-cloakwire-hwid-xray-subscriptions-design.md` only if implementation evidence requires a factual clarification
- No release publication in this task

**Interfaces:**
- Consumes all prior tasks.
- Produces a local, unpublished Windows installer/build and an evidence report containing versions, test counts, sidecar hashes, and redaction checks without secret values.

- [ ] **Step 1: Create a deterministic local mock-provider acceptance script**

The PowerShell script starts a local test server process that serves:

- valid link list;
- sing-box object and array;
- Xray ordinary and Observatory/balancer configs;
- reordered/removed child arrays;
- malformed, ambiguous, mixed, oversized, 401, 403, 410, 429, and 500 responses;
- allowlisted metadata headers.

The script prints only scenario names and pass/fail states, never request headers or response bodies.

- [ ] **Step 2: Run all static and unit gates**

Run:

```powershell
npm ci
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-targets -- --nocapture
& .\scripts\test-xray-sidecar.ps1
& .\scripts\test-hwid-xray-subscriptions.ps1
```

Expected: every command exits 0.

- [ ] **Step 3: Run a sensitive-value repository and log scan**

Search tracked files and generated normal logs for the known test token marker, test HWID, sample UUID, sample server address, and full mock URL. The test script must fail if any marker appears outside dedicated redacted test fixtures.

- [ ] **Step 4: Build an unpublished Windows installer from the feature branch**

Use a fresh `CARGO_TARGET_DIR` and the verified Xray sidecar. Run `npm run tauri:build`. Do not invoke `gh release`, do not tag, and do not modify `main`.

- [ ] **Step 5: Validate packaged sidecars and versions**

Extract or inspect the built NSIS/MSI payload and require both target-triple sidecars. Run the packaged copies with `sing-box version` and `xray version`; compare against expected pinned versions and compute SHA-256.

- [ ] **Step 6: Perform local Windows runtime smoke tests before user handoff**

Verify:

1. Existing manual share link connects through sing-box.
2. Existing link-list subscription connects through sing-box with Routing 2.0/TUN behavior unchanged.
3. Full Xray subscription appears once with all child profiles.
4. Ordinary and auto-selection Xray children validate and start.
5. System proxy is applied only after Xray starts and clears on stop, forced kill, failed validation, and app shutdown.
6. Provider routing, balancers, Observatory, tags, and fallback remain present after local compatible rules are prepended.
7. Process rules remain saved and visibly unavailable only in Xray mode.
8. Refresh rollback preserves the previous working bundle.
9. Normal logs contain no full URL, token, HWID, UUID, credential, or server address.

- [ ] **Step 7: Request independent verification**

Use `superpowers:requesting-code-review` and a verifier agent. The verifier checks the approved spec, every task commit, test evidence, installer inventory, runtime proxy cleanup, and secret redaction. Fix all confirmed high/medium findings before delivery.

- [ ] **Step 8: Commit validation tooling**

```powershell
git add scripts/test-hwid-xray-subscriptions.ps1 scripts/test-validate-release.ps1
git commit -m "test: add Windows HWID Xray acceptance gates"
```

- [ ] **Step 9: Provide the unpublished installer for user acceptance**

Send the verified installer and SHA-256 to the user with a short checklist. Explicitly state that macOS/Linux work and release publication remain blocked until the user approves Windows behavior.

---

## Self-Review Results

- Spec coverage: every requirement in sections 4-15 maps to Tasks 1-10; sing-box regression protection is explicit in Tasks 5 and 8, Xray routing precedence in Task 6, and Windows acceptance in Task 10.
- Secret boundary: URLs/full bundles/HWID are backend-only; frontend receives safe summaries and ephemeral parsed link outbounds only.
- Engine boundary: there is one process slot and one backend-selected engine; no UI core selector exists.
- Rollback boundary: refresh uses prepare/validate/commit, while runtime start clears system proxy on every failure/exit.
- Process routing: saved settings are never deleted; only Xray applicability is disabled and explained.
- Type consistency: `EngineKind`, `SubscriptionKind`, `ConnectionProfile`, `LaunchSpec`, `PreparedXrayConfig`, and `RoutingApplicability` retain the same names throughout all tasks.
- Placeholder scan: no implementation placeholders remain; the official Xray version, archive URL, archive member, archive SHA-256, executable SHA-256, and executable size are concrete, independently verified values, and the task defines strict validation and refusal behavior.
