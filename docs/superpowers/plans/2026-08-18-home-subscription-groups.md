# Home Subscription Groups Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Separate manually added servers from subscription-owned profiles on Home with compact collapsible subscription groups while preserving all existing selection and connection behavior.

**Architecture:** Keep `ConnectionProfile[]` as the single source of truth and preserve each profile's original flat index. Derive a frontend-only grouped view in `HomeTab` using an ID → safe display-name map passed from `App.tsx`; render the same grouped view in the dropdown picker and the quick-switcher strip. No backend commands, profile data shapes, or launch semantics change.

**Tech Stack:** React 18, TypeScript, Vitest, Tailwind utility classes, existing `lucide-react` icons and shared UI components.

## Global Constraints

- Change only Home presentation and focused frontend tests.
- Preserve the existing `ConnectionProfile[]` ordering and its integer selection index.
- Do not add backend commands, subscription URLs, endpoint data, credentials, or new WebView-visible metadata.
- Do not change Auto selection, sing-box, Xray, or connection startup behavior.
- Use only safe subscription summary names already available in `App.tsx`.
- Fresh Windows production builds must have `CLOAKWIRE_TEST_MANIFEST` explicitly empty.
- Do not commit generated `tsconfig.tsbuildinfo`.

---

### Task 1: Add safe subscription display-name plumbing

**Files:**
- Modify: `src/App.tsx: around the subscription hook and HomeTab render`
- Modify: `src/components/HomeTab.tsx: HomeTabProps and HomeTab destructuring`
- Test: `src/components/HomeTab.test.tsx`

**Interfaces:**
- Consumes: `subs.subs` from `useSubscriptions()` and each `SubscriptionSummary`'s `id`/`name`.
- Produces: `subscriptionNames: ReadonlyMap<string, string>` passed to `HomeTab` and then to `ServerPicker`.

- [ ] **Step 1: Write the failing helper test**

Add a small exported helper test in `HomeTab.test.tsx` for the grouping input contract: a map containing `subscription-1 → Work subscription` must be used as the group label, while a missing ID must fall back to `Subscription`.

```ts
it("uses safe subscription summary names for grouped labels", () => {
  expect(subscriptionGroupLabel("subscription-1", new Map([["subscription-1", "Work subscription"]]))).toBe("Work subscription");
  expect(subscriptionGroupLabel("missing", new Map())).toBe("Subscription");
});
```

Import `subscriptionGroupLabel` from `HomeTab.tsx`.

- [ ] **Step 2: Run the focused test and verify the new helper is absent**

Run from the repository root:

```powershell
npx vitest run src/components/HomeTab.test.tsx
```

Expected: the new test fails because `subscriptionGroupLabel` is not yet exported.

- [ ] **Step 3: Implement the safe map plumbing**

In `App.tsx`, derive the map without exposing URLs or metadata:

```ts
const subscriptionNames = new Map(subs.subs.map((subscription) => [subscription.id, subscription.name]));
```

Pass it to `<HomeTab subscriptionNames={subscriptionNames} />`.

In `HomeTabProps`, add:

```ts
subscriptionNames: ReadonlyMap<string, string>;
```

Destructure it and pass it into `ServerPicker`.

Export this helper from `HomeTab.tsx`:

```ts
export function subscriptionGroupLabel(
  subscriptionId: string,
  subscriptionNames: ReadonlyMap<string, string>,
): string {
  const name = subscriptionNames.get(subscriptionId)?.trim();
  return name || "Subscription";
}
```

- [ ] **Step 4: Run the focused test and verify it passes**

```powershell
npx vitest run src/components/HomeTab.test.tsx
```

Expected: PASS, with any pre-existing tests still passing.

- [ ] **Step 5: Commit the plumbing change**

```powershell
git add src/App.tsx src/components/HomeTab.tsx src/components/HomeTab.test.tsx
git commit -m "feat: expose safe subscription names on Home"
```

---

### Task 2: Add pure grouped-profile derivation with original indices

**Files:**
- Modify: `src/components/HomeTab.tsx`
- Modify: `src/components/HomeTab.test.tsx`

**Interfaces:**
- Consumes: flat `ConnectionProfile[]`.
- Produces: exported `groupHomeProfiles(profiles)` result containing manual rows and subscription groups, with every row retaining its original `index`.

- [ ] **Step 1: Add failing grouping tests**

Add tests covering manual rows, opaque subscription links, ready sing-box/Xray profiles, ordering, and original indexes:

```ts
it("groups Home profiles without changing their flat selection indexes", () => {
  const profiles: ConnectionProfile[] = [
    { kind: "manual", outbound: manualOutbound("Manual DE") },
    { kind: "subscription", reference: { subscription_id: "sub-a", link_key: "link-1" }, label: "Opaque NL", protocol: "vless" },
    { kind: "ready_config", subscriptionId: "sub-a", key: "xray-1", name: "France", engine: "xray" },
    { kind: "ready_config", subscriptionId: "sub-b", key: "sb-1", name: "Finland", engine: "singbox" },
  ];

  expect(groupHomeProfiles(profiles)).toEqual({
    manual: [{ index: 0, profile: profiles[0] }],
    subscriptions: [
      { id: "sub-a", rows: [{ index: 1, profile: profiles[1] }, { index: 2, profile: profiles[2] }] },
      { id: "sub-b", rows: [{ index: 3, profile: profiles[3] }] },
    ],
  });
});
```

Use a local test helper to create a valid minimal manual outbound, matching the existing test style.

- [ ] **Step 2: Run the focused test and verify it fails**

```powershell
npx vitest run src/components/HomeTab.test.tsx
```

Expected: FAIL because `groupHomeProfiles` is not implemented.

- [ ] **Step 3: Implement the pure grouping helper**

Add exported types and helper near the existing presentation helpers:

```ts
export type IndexedHomeProfile = { index: number; profile: ConnectionProfile };
export type HomeSubscriptionGroup = { id: string; rows: IndexedHomeProfile[] };
export type GroupedHomeProfiles = {
  manual: IndexedHomeProfile[];
  subscriptions: HomeSubscriptionGroup[];
};

export function groupHomeProfiles(profiles: ConnectionProfile[]): GroupedHomeProfiles {
  const manual: IndexedHomeProfile[] = [];
  const subscriptions: HomeSubscriptionGroup[] = [];
  const byId = new Map<string, HomeSubscriptionGroup>();

  profiles.forEach((profile, index) => {
    if (profile.kind === "manual") {
      manual.push({ index, profile });
      return;
    }

    const id = profile.kind === "subscription"
      ? profile.reference.subscription_id
      : profile.subscriptionId;
    let group = byId.get(id);
    if (!group) {
      group = { id, rows: [] };
      byId.set(id, group);
      subscriptions.push(group);
    }
    group.rows.push({ index, profile });
  });

  return { manual, subscriptions };
}
```

This preserves flat ordering within each group and first-seen subscription ordering.

- [ ] **Step 4: Run the focused test and verify it passes**

```powershell
npx vitest run src/components/HomeTab.test.tsx
```

Expected: PASS.

- [ ] **Step 5: Commit the grouping helper**

```powershell
git add src/components/HomeTab.tsx src/components/HomeTab.test.tsx
git commit -m "feat: group Home profiles by subscription"
```

---

### Task 3: Render grouped picker with disclosure state

**Files:**
- Modify: `src/components/HomeTab.tsx`
- Modify: `src/components/HomeTab.test.tsx` only if a focused interaction assertion is practical without introducing a new test harness

**Interfaces:**
- Consumes: `groupHomeProfiles`, `subscriptionNames`, existing display metadata and `onSelect(index)` callback.
- Produces: picker menu with Auto, expanded manual servers, and independently collapsible subscription groups.

- [ ] **Step 1: Implement picker grouping state**

Inside `ServerPicker`, derive:

```ts
const grouped = groupHomeProfiles(profiles);
const selectedSubscriptionId = selected?.kind === "subscription"
  ? selected.reference.subscription_id
  : selected?.kind === "ready_config"
    ? selected.subscriptionId
    : null;
const [expandedSubscriptions, setExpandedSubscriptions] = useState<Set<string>>(new Set());
```

When opening the picker, initialize the set to include `selectedSubscriptionId` and no other subscriptions. Use a functional state update so reopening remains deterministic after profile refreshes.

- [ ] **Step 2: Render the existing Auto row unchanged**

Keep the current Auto row at the top, including `onSelect(-1)`, close-on-select behavior, and existing styling.

- [ ] **Step 3: Render manual rows in an expanded Servers section**

Render a compact non-interactive section label only when `grouped.manual.length > 0`, then map `grouped.manual` through the existing profile-row presentation. Each row must call `onSelect(row.index)` and close the picker.

- [ ] **Step 4: Render subscription disclosure groups**

For each `grouped.subscriptions` entry, render a real button with:

- `aria-expanded` matching whether its ID is expanded;
- accessible label containing `subscriptionGroupLabel(group.id, subscriptionNames)` and row count;
- truncated group name and count;
- Chevron icon rotated when expanded.

When a group header is clicked, toggle only that ID. When expanded, render its rows using the existing `connectionProfileDisplay` and latency/flag presentation, preserving `row.index`.

- [ ] **Step 5: Preserve picker accessibility and close behavior**

Keep the existing outside-click and Escape listeners. Profile rows remain keyboard-accessible buttons. Selecting any profile closes the picker exactly as before.

- [ ] **Step 6: Run the focused tests and build**

```powershell
npx vitest run src/components/HomeTab.test.tsx
npm run build
```

Expected: PASS; build completes with only the existing Vite chunk-size warning if present.

- [ ] **Step 7: Commit the picker change**

```powershell
git add src/components/HomeTab.tsx src/components/HomeTab.test.tsx
git commit -m "feat: collapse subscription groups in Home picker"
```

---

### Task 4: Render the grouped quick-switcher strip

**Files:**
- Modify: `src/components/HomeTab.tsx`
- Modify: `src/components/HomeTab.test.tsx` only if a pure helper test is needed

**Interfaces:**
- Consumes: `groupHomeProfiles`, `subscriptionNames`, current `selectedIndex`, existing display helpers.
- Produces: manual server tiles immediately visible and collapsible subscription cards below them.

- [ ] **Step 1: Add local disclosure state for the quick-switcher**

In `HomeTab`, derive `groupedProfiles = groupHomeProfiles(profiles)` and maintain a `Set<string>` of expanded subscription IDs. Initialize the set to the subscription containing `selectedIndex`; keep manual profiles always visible. Reconcile the set when profiles change so removed subscriptions do not leave stale IDs.

- [ ] **Step 2: Replace the flat `profiles.map` grid**

Keep the outer card and existing `Servers (N)` heading. Render manual tiles only when present, using each row's original index.

Render a `Subscriptions (M)` subsection only when there are groups. Each group header is a disclosure button with name, profile count, and `aria-expanded`. Expanded groups render the same tile markup and preserve the original index in `onSelect(row.index)`.

- [ ] **Step 3: Keep the no-subscription and no-manual cases clean**

Do not render empty headings. If only subscription groups exist, show only the subscription subsection. If only manual profiles exist, the UI should remain visually equivalent to the current flat grid.

- [ ] **Step 4: Run the focused tests, build, and diff check**

```powershell
npx vitest run src/components/HomeTab.test.tsx
npm run build
git diff --check HEAD~1..HEAD
```

Expected: PASS; no whitespace errors.

- [ ] **Step 5: Commit the quick-switcher change**

```powershell
git add src/components/HomeTab.tsx src/components/HomeTab.test.tsx
git commit -m "feat: group subscription servers in Home switcher"
```

---

### Task 5: Build the test Windows installer for manual validation

**Files:**
- No source files expected beyond the commits above.
- Build output: external `C:\Users\Public\cwdev\target\release\bundle\nsis\Cloakwire_1.3.0_x64-setup.exe`
- Logs: external `C:\Users\Public\cwdev\build-logs\home-subscription-groups-build.stdout.log` and `.stderr.log`

**Interfaces:**
- Consumes: verified source branch with Home grouping commits.
- Produces: fresh unsigned NSIS installer for the user to install and manually test.

- [ ] **Step 1: Confirm source state before building**

```powershell
$ErrorActionPreference = 'Stop'
Set-Location 'C:\Users\Public\cwdev\cloakwire-hwid-xray'
git status --short --branch
$env:CLOAKWIRE_TEST_MANIFEST = ''
```

Expected: only the pre-existing generated `tsconfig.tsbuildinfo` remains modified.

- [ ] **Step 2: Build the production frontend once through the Tauri build**

Use the existing ASCII-safe target directory and detached logs. Run the same verified Windows build path used for the previous installer, with `CLOAKWIRE_TEST_MANIFEST` explicitly empty. Do not add test manifests, credentials, or release staging files.

- [ ] **Step 3: Verify the fresh NSIS artifact**

Confirm exactly one installer exists, record its timestamp, size, and SHA-256, and confirm the generated files are outside git tracking.

- [ ] **Step 4: Report the installer path and checksum**

Tell the user the build is ready for manual Windows testing. Do not merge into `main`, create a release tag, or build macOS/Linux artifacts until the user confirms the UI works.
