# Cloakwire HWID and Full-Config Subscription Design

Date: 2026-08-17
Status: approved for implementation on 2026-08-17
Initial platform: Windows
Base commit: `bef8713abd23a2546d9c8a0aa62af2f4d4c8f0a1`
Branch: `feature/hwid-xray-subscriptions`

## 1. Goal

Extend Cloakwire subscriptions so the application can:

- send a stable device identifier in `X-HWID`;
- receive subscription metadata from HTTP headers;
- import subscription responses containing ready JSON configurations;
- preserve one subscription containing multiple named configurations;
- refresh that subscription in place without creating duplicates;
- keep sing-box as the primary engine and preserve every existing sing-box feature;
- use Xray-core only when a payload cannot be executed through sing-box without losing required behavior.

The first implementation and user acceptance test are Windows-only. macOS and Linux work begins only after the Windows behavior is approved. Android is outside this implementation because the trusted Android source provenance is still unresolved.

## 2. Confirmed provider behavior

A real subscription was tested with a Cloakwire user agent and a stable test `X-HWID`. Sensitive URL tokens, UUIDs, server addresses, and credential values are not recorded in this document.

The endpoint returned:

- HTTP 200;
- `Content-Type: application/json`;
- a root JSON array containing seven independent configurations;
- metadata headers including `Profile-Title`, `Profile-Update-Interval`, `Profile-Web-Page-Url`, `Support-Url`, and `Subscription-Userinfo`;
- Xray configuration objects with `outbounds[].protocol`, `routing.domainStrategy`, `routing.balancers`, and `observatory`;
- VLESS, Reality/TLS, TCP/XHTTP, server routing rules, and a least-latency auto-selection configuration.

The endpoint did not send its client-specific `Routing` header to `User-Agent: Cloakwire`, which is expected because that header is currently reserved for other named clients.

## 3. Non-goals

This work does not:

- replace sing-box with Xray;
- change the behavior of existing share-link subscriptions;
- remove or downgrade Routing 2.0, profile selection, system proxy, TUN, logs, updates, or other sing-box features;
- implement a universal Xray-to-sing-box converter;
- attempt lossy conversion of Observatory, Xray balancers, or arbitrary Xray routing;
- add Windows process-aware routing to Xray through WFP, tun2socks, or another kernel/network layer;
- implement macOS, Linux, or Android support before Windows acceptance;
- publish a release before local Windows testing succeeds.

## 4. Core principle: capability-based engine selection

Cloakwire chooses an engine automatically. The user does not select a core manually.

Selection order:

1. Existing supported share links and link-list subscriptions continue through the current parser, config generator, and sing-box runtime.
2. A ready sing-box configuration is validated and run by sing-box.
3. A ready Xray configuration is evaluated for lossless sing-box compatibility.
4. If Cloakwire has an explicit, tested, lossless adapter for every required semantic element, it may run the converted profile through sing-box.
5. Otherwise the original ready configuration runs through Xray-core.

For the first Windows release, arbitrary full Xray configurations are classified as Xray profiles. They are not converted because Xray `observatory`, `routing.balancers`, and complete routing behavior cannot be preserved by a generic conversion.

This is a strict fallback boundary: every link, subscription, or ready configuration that Cloakwire can execute through sing-box without semantic loss continues to use sing-box and retains all existing sing-box features. Xray is selected only for payloads that sing-box cannot support losslessly; in that mode Cloakwire follows Xray's own configuration and routing capabilities rather than pretending that sing-box-only features are available.

This rule keeps sing-box primary while ensuring that adding Xray does not silently alter a provider's intended behavior.

## 5. Subscription request contract

The Rust backend performs the network request. The frontend must not fetch full-config subscriptions directly.

Each request sends:

- `User-Agent: Cloakwire/<app-version> (<platform>)`;
- `X-HWID: <stable-random-device-id>`;
- `X-Device-OS: Windows` for the first platform implementation;
- `X-Device-Model: Cloakwire Desktop`;
- `Accept: application/json, text/plain`.

Privacy rules:

- Cloakwire generates a random UUID on first use;
- the HWID is stable across app restarts and normal upgrades;
- raw hardware serials, MAC addresses, Windows product IDs, usernames, and machine names are not used;
- changing or resetting the HWID requires an explicit user action with a warning that the provider may count it as a new device;
- request URLs, tokens, HWIDs, UUIDs, and complete response bodies are never written to normal application logs.

Network limits:

- HTTPS is required for non-localhost full-config subscriptions;
- redirects are allowed only to HTTP(S), with HTTPS-to-HTTP downgrade rejected except for explicit localhost development;
- request timeout is 30 seconds;
- response size is capped at 10 MiB;
- malformed, oversized, or unsupported responses fail without replacing the last valid subscription state.

## 6. Response classification

Classification uses content and structure, not provider hostname or hardcoded URL patterns.

Supported response classes:

### 6.1 Existing link list

Plain text or base64 text containing supported share links. It follows the existing sing-box path unchanged.

### 6.2 Ready sing-box config

A JSON object, or an array of objects, whose configuration structure uses sing-box markers such as `outbounds[].type`. Each valid object becomes one configuration profile backed by sing-box.

### 6.3 Ready Xray config

A JSON object, or an array of objects, whose configuration structure uses Xray markers such as `outbounds[].protocol`, `routing.domainStrategy`, `observatory`, or `routing.balancers`. Each valid object becomes one configuration profile backed by Xray unless a future explicit lossless sing-box adapter accepts it.

### 6.4 Unsupported or ambiguous payload

If the payload is valid JSON but engine classification is ambiguous, Cloakwire rejects it with a diagnostic summary. It must not guess and must not run unknown JSON as a VPN configuration.

A mixed array containing configs for different engines is rejected in the first release. One subscription response must resolve to one engine class.

## 7. Persistence model

Existing subscription entries remain backward compatible. An old entry without a `kind` field is treated as `auto` and uses the current link-list behavior until refreshed and classified.

The subscription model gains:

- `kind`: `auto | link_list | singbox_bundle | xray_bundle`;
- `engine`: `singbox | xray | null`;
- `profileTitle`;
- `profileWebPageUrl`;
- `supportUrl`;
- parsed subscription user information;
- provider update interval;
- last successful refresh timestamp;
- last HTTP status and typed error category;
- active child profile key;
- child profile summaries.

Full JSON configurations and credential-bearing URLs are persisted by the Rust backend under the application data directory, not duplicated into React state persistence or browser logs. Files are written atomically. The previous valid bundle remains available until the complete replacement bundle has been fetched, parsed, validated, and committed.

Child profiles use a stable key derived from non-secret provider identity fields. Preferred identity order:

1. an explicit provider profile ID when present;
2. normalized `remarks` or profile name plus its duplicate ordinal;
3. array position as the final fallback.

Credential values such as UUIDs are excluded from identity so a normal credential rotation updates the existing profile instead of creating a duplicate.

## 8. Runtime architecture

### 8.1 Engine abstraction

Introduce a small engine boundary rather than rewriting the whole application around Xray.

The boundary exposes:

- locate binary;
- report version;
- validate config;
- start config;
- stop;
- status;
- logs;
- active engine and active profile.

The existing sing-box implementation remains the default implementation. Xray receives a separate implementation behind the same boundary. Only one engine may run at a time.

Switching profiles or engines performs:

1. stop the currently running engine;
2. restore or clear the previous Windows system proxy state;
3. build and validate the new runtime config;
4. start the selected engine;
5. apply the new system proxy only after successful startup;
6. restore safe network state if startup fails.

### 8.2 Xray binary

The Windows package includes an official Xray-core executable as a second sidecar. The implementation pins an explicit upstream version and SHA-256 digest in repository-controlled build metadata. Download or packaging code verifies the digest. Cloakwire never downloads and executes an unverified binary.

The first release does not implement automatic Xray-core updates. Xray is updated with Cloakwire releases until a separately designed signed updater exists.

### 8.3 Windows traffic interception

The first Xray implementation uses Windows system proxy mode. It does not add a TUN/WFP layer.

Cloakwire selects a loopback HTTP inbound from the provider config when it is safe and unambiguous. If no usable loopback HTTP inbound exists, Cloakwire adds a runtime-only managed HTTP inbound with a collision-free local port and a reserved tag. The stored provider config remains unchanged.

The system proxy is applied only after Xray starts successfully. It is cleared on stop, crash, failed health check, or application shutdown using the same safety principles as the sing-box process manager.

A provider configuration that cannot safely expose a local proxy inbound is rejected with a clear error rather than started in a state where the UI claims the VPN is active but traffic is not intercepted.

## 9. Routing merge policy

### 9.1 Sing-box profiles

No behavior changes. Routing 2.0 continues to use the existing sing-box generator, including process-name rules and all current features.

### 9.2 Xray profiles

The server's original Xray routing remains authoritative for semantics that Cloakwire cannot represent safely. Cloakwire creates a runtime copy and prepends only local Routing 2.0 rules that have an exact Xray equivalent.

Rule order:

1. compatible enabled Cloakwire custom rules;
2. original provider `routing.rules` in their original order;
3. original provider fallback behavior.

Cloakwire does not replace the provider's balancers, Observatory configuration, outbound tags, or final routing behavior.

Supported first-release local matcher mappings are limited to exact Xray equivalents:

- domain exact, suffix, keyword, and supported geosite expressions;
- IP/CIDR and supported geoip expressions;
- destination port or port range;
- network (`tcp`, `udp`, or both);
- protocol values supported by Xray routing;
- inbound tag when explicitly present and valid.

Supported actions are limited to routing to an existing outbound tag or balancer tag represented in the provider config, plus blocking when a valid blackhole outbound exists or can be safely injected.

Rules with no exact mapping are not silently discarded. The Routing UI marks them as not applicable to the active Xray profile and explains why.

### 9.3 Process-name limitation

Xray routing has no direct equivalent to sing-box `process_name`. Therefore `Apps direct` and `Apps via VPN` remain fully functional for sing-box profiles but are not applied to Xray profiles in the first Windows release.

This is a capability difference, not a global removal. The UI shows the active engine and an explicit notice when process-based rules are unavailable. Adding Xray process routing would require a separate Windows TUN/WFP design and is outside this patch.

## 10. UI behavior

The interface remains centered on profiles, not cores.

Subscription UI additions:

- detected type: Links, sing-box configs, or Xray configs;
- engine badge on each child profile;
- provider title and support/web links when present;
- expiry and traffic information parsed from `Subscription-Userinfo`;
- provider-requested refresh interval;
- current HWID display with copy and explicit reset actions;
- typed errors for authentication, expired access, device limit, malformed payload, unsupported config, network failure, and validation failure.

Profile selection:

- one subscription may expose multiple child profiles;
- selecting a child profile does not duplicate the subscription;
- refresh retains the selected profile by stable key when possible;
- if the selected profile disappears, Cloakwire selects the first valid profile and informs the user;
- the active engine is shown but is chosen automatically.

Routing UI:

- sing-box profiles behave exactly as today;
- Xray profiles show which rules will be merged and which are unavailable;
- process-based app selectors remain saved and resume automatically when the user returns to a sing-box profile.

## 11. Error handling and rollback

The refresh pipeline is transactional:

1. fetch response;
2. capture allowlisted metadata headers;
3. classify payload;
4. parse all child configs;
5. validate each config with its selected engine;
6. build merged runtime candidates where applicable;
7. atomically replace stored bundle only if every required step succeeds.

Failure rules:

- a failed refresh never deletes the last valid bundle;
- a partially valid JSON array is rejected as a whole in the first release;
- validation errors identify the child profile by sanitized display name;
- provider error bodies are not logged verbatim;
- HTTP 401/403/404/410/429 and 5xx responses receive distinct error categories;
- device-limit and expiry messages may be shown after token/HWID/UUID redaction;
- Xray or sing-box startup failure restores the previous safe proxy state;
- Cloakwire never falls back from Xray to a lossy sing-box conversion automatically.

## 12. Compatibility and migration

Existing users must retain:

- manual profiles;
- existing share-link subscriptions;
- stored refresh intervals;
- profile selection behavior;
- Routing 2.0 settings;
- process routing for sing-box;
- TUN/system-proxy choices for sing-box;
- logs and updater behavior.

Migration is additive. Missing new fields receive defaults. Existing subscription URLs are not rewritten, reclassified, or re-fetched solely because the application upgraded.

Xray support is feature-gated by successful Xray binary discovery and version validation. If Xray is unavailable, sing-box continues to work and Xray profiles show an actionable unavailable-engine error.

## 13. Testing strategy

### 13.1 Unit tests

- stable random HWID generation and persistence;
- header construction without leaking secrets;
- response size and redirect enforcement;
- link-list, sing-box, Xray, ambiguous, and malformed classification;
- metadata header parsing;
- subscription model migration;
- child profile stable-key matching;
- transactional refresh rollback;
- routing translation for every supported matcher/action;
- explicit rejection or warning for unsupported process rules;
- runtime inbound selection/injection;
- sensitive-value redaction.

### 13.2 Integration tests

Use a local mock HTTP server to cover:

- valid link lists;
- one and multiple sing-box configs;
- one and multiple Xray configs;
- HWID-required responses;
- device limit, expiry, unauthorized, rate limit, and server failures;
- metadata headers;
- malformed and oversized bodies;
- changed credentials with stable profile identity;
- removed and reordered profiles;
- refresh rollback after validation failure.

### 13.3 Windows acceptance gates

Before asking the user to test:

- frontend typecheck and production build pass;
- Rust tests pass;
- both bundled cores report expected versions;
- known sing-box subscriptions behave unchanged;
- the tested HWID subscription imports as one subscription with seven child profiles;
- each Xray child configuration passes Xray validation;
- at least one ordinary profile and the auto-selection profile start successfully;
- Windows system proxy is applied only while connected and restored after stop/crash;
- provider routing remains present after Cloakwire-compatible rules are prepended;
- unsupported Apps routing is visibly reported in Xray mode;
- no token, HWID, UUID, or server credential appears in normal logs.

The user then performs local Windows functional testing. Only after explicit acceptance does the project proceed to macOS/Linux implementation and a new release.

## 14. Delivery sequence

1. Record user approval and commit this design specification.
2. Write a detailed implementation plan.
3. Implement backend subscription fetching, HWID storage, classification, persistence, and tests.
4. Implement the engine abstraction and Windows Xray runtime.
5. Implement routing translation and UI capability reporting.
6. Package and validate the Windows application.
7. Provide the Windows build for local user testing.
8. Fix Windows findings without starting cross-platform work prematurely.
9. After Windows approval, design and implement macOS/Linux packaging and runtime behavior.
10. Independently verify release artifacts before publishing a new version.

## 15. Acceptance criteria

The design is implemented successfully when:

- sing-box remains the default and existing behavior passes regression tests;
- supported links and subscriptions continue to use sing-box;
- full Xray bundles are accepted with stable HWID requests and run through Xray without lossy conversion;
- one remote subscription can expose and update multiple child profiles;
- provider metadata is preserved and displayed safely;
- compatible Routing 2.0 rules are merged before provider Xray rules;
- unsupported process-name rules remain intact for sing-box and are clearly marked unavailable for Xray;
- failed refreshes and failed core starts preserve a safe, recoverable state;
- sensitive subscription data is not exposed in logs;
- Windows is tested and approved before other platforms or release publication.
