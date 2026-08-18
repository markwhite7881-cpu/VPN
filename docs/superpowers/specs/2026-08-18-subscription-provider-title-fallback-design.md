# Subscription display names: provider-title fallback

**Date:** 2026-08-18
**Status:** approved design — awaiting specification review

## Goal

Avoid indistinguishable Home groups when several subscriptions were added without a manual name. A provider-supplied display title should replace only the generic stored name `Subscription`.

## Scope

- Change subscription-name persistence and focused tests only.
- Keep the existing Home grouping model: groups remain keyed by internal subscription ID, never by displayed name.
- Do not expose subscription URLs, raw payloads, endpoint data, credentials, opaque configuration, or new provider metadata to the WebView.
- Do not change connection selection, Auto, sing-box, Xray, refresh intervals, or startup behavior.

## Design

### Name precedence

A subscription record has a persisted `name` and a fetched safe `metadata.profile_title` parsed from the HTTP `Profile-Title` header.

On the first successful fetch and on later refreshes:

1. A non-generic persisted name is authoritative and remains unchanged.
2. If the persisted name is exactly the default `Subscription` (after trimming) and the fetched `profile_title` is non-empty after trimming, persist that title as the record name.
3. If no usable provider title is returned, retain `Subscription`.

This supports existing unnamed records during their next successful refresh and new unnamed records immediately after their initial fetch. A manually chosen name is never overwritten by a provider.

### Presentation

`SubscriptionSummary.name` continues to be the only group-label input sent to Home. `App.tsx` keeps mapping the existing summary `id` and `name` into `subscriptionNames`; no provider metadata needs to be added to Home props.

Two subscriptions with equal provider titles remain separate disclosure groups because `groupHomeProfiles` groups by each subscription's stable internal ID. Equal visible titles are possible only when the provider itself returns duplicate titles or the user deliberately assigns the same manual name; profiles never merge.

## Error handling

- A failed refresh does not alter the stored name.
- An absent, blank, or whitespace-only `Profile-Title` does not alter the stored name.
- Existing validation continues to reject blank manually supplied names at the backend boundary; the frontend default remains `Subscription` for backwards compatibility.

## Tests

Add focused Rust service tests proving:

1. A generic `Subscription` record is renamed to a non-empty fetched `Profile-Title`.
2. A custom persisted name remains unchanged even when a fetched title differs.
3. A blank or missing fetched title leaves `Subscription` unchanged.

Retain the existing Home grouping tests, which already prove grouping uses subscription IDs and presentation uses the persisted safe summary name.

## Verification

1. Run the focused subscription Rust tests (compile-only fallback if this host's Rust test runner remains blocked by `STATUS_ENTRYPOINT_NOT_FOUND`).
2. Run `npx vitest run src/components/HomeTab.test.tsx`.
3. Run `npm run build`.
4. Run `git diff --check`.
5. If application code changes, rebuild the unsigned Windows installer with `CLOAKWIRE_TEST_MANIFEST` explicitly empty before manual installation validation.
