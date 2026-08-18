# Subscription provider title fallback Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Use a safe provider `Profile-Title` as the subscription name only when the stored name is the generic `Subscription`, keeping manually chosen names authoritative.

**Architecture:** The existing HTTP client already parses the allowlisted `Profile-Title` header into `FetchedPayload.metadata.profile_title`. `SubscriptionService::prepare_candidate` will copy that safe metadata and normalize the candidate record name before classification; the existing `to_summary()` and Home name map will then expose the resolved name without new frontend props or metadata paths. Tests will exercise the real local HTTP test server and persisted service flow.

**Tech Stack:** Rust, Tokio, existing `SubscriptionService`, `SubscriptionHttpClient`, tempfile-backed `SubscriptionStore`, existing async subscription tests.

## Global Constraints

- Change only subscription-name persistence and focused tests.
- Keep Home groups keyed by internal subscription ID, never by displayed name.
- Do not expose subscription URLs, raw payloads, endpoint data, credentials, opaque configuration, or new provider metadata to the WebView.
- Do not change connection selection, Auto, sing-box, Xray, refresh intervals, or startup behavior.
- A non-generic persisted name must never be overwritten by a provider.
- A failed refresh or blank/missing `Profile-Title` must preserve the current stored name.
- Do not commit generated `tsconfig.tsbuildinfo`.

---

### Task 1: Add failing provider-title persistence tests

**Files:**
- Modify: `src-tauri/src/subscriptions/tests.rs` near the existing service refresh tests
- Read: `src-tauri/src/subscriptions/service.rs` for `prepare_candidate` and `add` flow

**Interfaces:**
- Consumes: existing `SubscriptionService::add`, `SubscriptionService::refresh`, `http_response`, `spawn_sequence_server`, and `service_with_responses` test helpers.
- Produces: three regression tests that define the exact name precedence contract.

- [ ] **Step 1: Add the generic-name fallback test**

Add an async test using a local HTTP response containing an allowlisted `Profile-Title` header and a valid link-list body. Add the subscription with `name: "Subscription"`, then assert both the returned summary and a subsequent `service.list()` summary use the provider title:

```rust
#[tokio::test]
async fn provider_title_replaces_generic_subscription_name() {
    let response = http_response_with_headers(
        "text/plain",
        &[('Profile-Title', "Anivka" )],
        b"vless://11111111-1111-4111-8111-111111111111@server-address.example.test:443?security=tls#ProviderLabel",
    );
    let (service, url, server) = service_with_responses(vec![response]).await;

    let added = service
        .add(AddSubscriptionInput {
            name: "Subscription".into(),
            url,
            interval_minutes: 60,
        })
        .await
        .unwrap();

    assert_eq!(added.subscription.name, "Anivka");
    assert_eq!(service.list().await.unwrap().subscriptions[0].name, "Anivka");
    let _ = server.await;
}
```

Use the repository's existing response helpers and exact Rust syntax/types already used in `tests.rs`; if a header helper does not exist, extend the local test-only response builder without logging or returning credentials.

- [ ] **Step 2: Add the custom-name preservation test**

Add a test with the same provider header but `name: "My work subscription"`. Assert both add result and persisted list retain `My work subscription`, proving provider metadata is not authoritative over a manual name.

- [ ] **Step 3: Add the blank-title preservation test**

Add a test whose response omits `Profile-Title` or sends only whitespace. Add with `name: "Subscription"` and assert the stored name remains exactly `Subscription`.

- [ ] **Step 4: Run the focused Rust tests and verify the new tests fail**

Run from `C:\Users\Public\cwdev\cloakwire-hwid-xray\src-tauri`:

```powershell
cargo test subscriptions::tests::provider_title_replaces_generic_subscription_name --lib
cargo test subscriptions::tests::custom_subscription_name_wins_over_provider_title --lib
cargo test subscriptions::tests::blank_provider_title_keeps_generic_subscription_name --lib
```

Expected: the tests compile and fail because `prepare_candidate` currently copies provider metadata but never changes `candidate.name`.

---

### Task 2: Implement minimal name precedence in the service

**Files:**
- Modify: `src-tauri/src/subscriptions/service.rs:320-333`
- Test: `src-tauri/src/subscriptions/tests.rs`

**Interfaces:**
- Consumes: `SubscriptionRecord.name`, `FetchedPayload.metadata.profile_title`, and the existing candidate preparation path used by add, refresh, and legacy migration.
- Produces: candidate records whose `name` follows the approved precedence rules before `to_summary()` is called.

- [ ] **Step 1: Add a small private helper with explicit precedence**

Near the existing service helpers, add:

```rust
fn apply_provider_title_fallback(record: &mut SubscriptionRecord) {
    if record.name.trim() != "Subscription" {
        return;
    }
    let Some(title) = record
        .metadata
        .profile_title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
    else {
        return;
    };
    record.name = title.to_owned();
}
```

- [ ] **Step 2: Call the helper after fetched metadata is assigned**

In `prepare_candidate`, keep the existing metadata assignment and immediately apply the fallback:

```rust
let mut candidate = current.clone();
candidate.metadata = payload.metadata;
apply_provider_title_fallback(&mut candidate);
candidate.last_http_status = Some(payload.status);
```

This applies consistently to initial add, explicit refresh, automatic refresh, and legacy migration, while failed requests still return before any stored record is replaced.

- [ ] **Step 3: Run the three focused tests and verify they pass**

Run:

```powershell
cargo test subscriptions::tests::provider_title_replaces_generic_subscription_name --lib
cargo test subscriptions::tests::custom_subscription_name_wins_over_provider_title --lib
cargo test subscriptions::tests::blank_provider_title_keeps_generic_subscription_name --lib
```

Expected: all three pass, subject to the known Windows Rust runtime issue `0xc0000139`; if that issue occurs, retain the compile/test evidence and use the repository's compile-only fallback.

- [ ] **Step 4: Run the complete subscription test module**

Run:

```powershell
cargo test subscriptions --lib
```

Expected: existing subscription security, refresh, metadata, and selection tests remain green or are blocked only by the known host runtime entrypoint issue.

- [ ] **Step 5: Commit the implementation and tests**

```powershell
git add src-tauri/src/subscriptions/service.rs src-tauri/src/subscriptions/tests.rs
git commit -m "feat: use provider title for unnamed subscriptions"
```

Do not stage `tsconfig.tsbuildinfo`.

---

### Task 3: Verify frontend compatibility and release impact

**Files:**
- Read-only verification: `src/App.tsx`, `src/components/HomeTab.tsx`, `src/components/HomeTab.test.tsx`
- No frontend source changes expected.

**Interfaces:**
- Consumes: persisted `SubscriptionSummary.name` from the backend.
- Produces: evidence that existing Home grouping displays the resolved name and still isolates groups by subscription ID.

- [ ] **Step 1: Run the focused Home tests**

From the repository root:

```powershell
npx vitest run src/components/HomeTab.test.tsx
```

Expected: all existing Home grouping tests pass.

- [ ] **Step 2: Run the production frontend build**

```powershell
npm run build
```

Expected: build succeeds without changing frontend bundles or interfaces.

- [ ] **Step 3: Check whitespace and repository status**

```powershell
git diff --check
git status --short
```

Expected: no whitespace errors; only intended source/test commits are present, while generated `tsconfig.tsbuildinfo` remains unstaged/uncommitted.

- [ ] **Step 4: Rebuild the unsigned Windows installer if application code changed**

Use the existing release command with `CLOAKWIRE_TEST_MANIFEST` explicitly empty. Verify the installer exists and record its SHA-256 before manual validation. Do not publish or move any release tag as part of this focused change.
