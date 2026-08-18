# Home: grouped manual servers and subscriptions

**Date:** 2026-08-18
**Status:** approved design — awaiting specification review

## Goal

Make the Home screen clearly separate manually added servers from subscription-owned profiles without changing the backend, connection-selection semantics, or safety boundary. The UI must remain compact when several subscriptions are present.

## Scope

- Change only Home presentation and focused frontend tests.
- Preserve the existing `ConnectionProfile[]` ordering and its integer selection index.
- Do not add backend commands, subscription URLs, endpoint data, credentials, or new WebView-visible metadata.
- Do not change Auto selection, sing-box, Xray, or connection startup behavior.

## User experience

### Shared grouping model

A frontend-only helper will derive two view groups from the existing flat `profiles` list. `HomeTab` will receive an ID → display-name map derived in `App.tsx` from the already rendered safe subscription summaries; it will not receive subscription URLs or raw provider metadata.

1. **Manual servers** — `profile.kind === "manual"`.
2. **Subscriptions** — every `subscription` or `ready_config` profile, grouped by its existing subscription ID and displayed in snapshot order. The ID → display-name map provides each group heading; an unavailable name falls back to the neutral label **Subscription**.

The grouping retains the original profile index beside every displayed profile. Selecting a row calls the current `onSelect(originalIndex)` callback, leaving all launch logic unchanged.

### Server picker above the connect button

The existing picker continues to show the selected server and latency. Its menu contains:

- The existing **Auto** item at the top.
- A **Servers** section for manual profiles, shown expanded.
- A **Subscriptions** section when subscription groups exist.
- One compact, collapsible row per subscription showing its display name and count of available profiles.

Subscription rows are collapsed initially to prevent a long menu. The group containing the selected profile is automatically expanded whenever the picker opens. A user can expand or collapse any group manually during that interaction.

Each child row keeps the existing flag, friendly label, protocol/engine presentation, latency when safe metadata already provides it, and selection highlighting. No source URL, endpoint, raw configuration, or provider data is shown.

### All-servers strip below the status cards

The current flat `Servers (N)` grid becomes the same two-level layout:

- **Servers (N)** contains manual server tiles when any exist.
- **Subscriptions (M)** contains a compact collapsible card per subscription.
- Each expanded subscription group renders the existing server tiles using their original selection indices.

Manual profiles remain visible immediately. Subscription groups start collapsed unless they contain the selected profile; that group starts expanded. A subscription with no rendered profile is omitted.

## Edge cases

- **No manual servers:** show the subscriptions section alone; Auto remains available only under the existing capability rule.
- **No subscriptions:** preserve an effectively identical manual-server UI without empty headings.
- **Multiple subscriptions:** each group is independently expandable; all names truncate safely and the scrollable picker retains its maximum height.
- **Selected subscription profile:** its group stays discoverable by opening it automatically; selection styling remains accurate.
- **Opaque subscription links and ready configs:** both stay inside their owning subscription group. Existing safe metadata drives flags/latency for ready Xray profiles only.
- **Profile refresh/removal:** grouping is recomputed from `profiles` every render; existing index-clamping logic remains responsible for invalid selection repair.

## Components and responsibilities

- `HomeTab.tsx`: owns the view-group derivation and passes grouped rows into the picker and the quick-switcher.
- `ServerPicker`: renders Auto, manual section, subscription disclosure controls, and profile rows; it does not alter selection semantics.
- A small presentational section/group component may be extracted within `HomeTab.tsx` if it prevents duplicated row rendering.
- `HomeTab.test.tsx`: covers grouping, original-index preservation, grouping of both subscription profile kinds, and selected-subscription expansion policy. Existing display-helper tests remain unchanged.

## Accessibility and interaction

- Disclosure controls are real buttons with `aria-expanded` and descriptive labels containing the subscription name and profile count.
- Profile controls retain keyboard operability and visible focus styles.
- Clicking a subscription header only changes disclosure state; clicking a profile selects it and closes the picker, as today.
- Escape/outside click continues to close the picker.

## Verification

Targeted checks only before the Windows test installer:

1. Focused unit tests for grouping and Home presentation.
2. `npm run build`.
3. `git diff --check`.
4. Fresh unsigned Windows production NSIS build with `CLOAKWIRE_TEST_MANIFEST` explicitly empty.

The user will manually test the Windows installer before any integration into `main`, cross-platform artifacts, tag creation, or GitHub Release publication.
