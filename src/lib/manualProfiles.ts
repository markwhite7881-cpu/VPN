// Persistence for manually-added server profiles.
//
// `manualProfiles` lives in `App.tsx` as React `useState`, which
// means it's reset to `[]` on every app launch. For a regular user
// who pastes a `vless://...` link and expects the server to still
// be there next time they open the app, that's a UX gap — they
// have to find and re-paste the link every time.
//
// Subscriptions are already persisted (via `useSubscriptions`).
// This module persists the OTHER source of profiles: the ones the
// user added by hand.
//
// Persistence strategy: JSON.stringify the array on every change
// and stash it in `localStorage` under a single v1 key. On launch,
// loadFromStorage() validates the shape and silently drops anything
// corrupt (including the whole array) so a malformed cache can't
// brick the app.

import type { Outbound } from "./types";

const STORAGE_KEY = "singbox-client.manual-profiles.v1";

/** The set of `protocol` values we recognise. Anything else is
 *  dropped on load — it's either a future protocol we don't
 *  support yet, or a corrupted entry. */
const VALID_PROTOCOLS = new Set<Outbound["protocol"]>([
  "vless",
  "vmess",
  "trojan",
  "shadowsocks",
  "hysteria2",
  "tuic",
  "unsupported",
]);

/** Load manual profiles from localStorage. Returns `[]` on any
 *  failure (missing, malformed, wrong shape). Never throws. */
export function loadManualProfiles(): Outbound[] {
  if (typeof window === "undefined") return [];
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((item): item is Outbound => {
      if (!item || typeof item !== "object") return false;
      const o = item as { protocol?: unknown; tag?: unknown };
      return (
        typeof o.protocol === "string" &&
        VALID_PROTOCOLS.has(o.protocol as Outbound["protocol"]) &&
        typeof o.tag === "string" &&
        o.tag.length > 0
      );
    });
  } catch {
    /* corrupt cache — start fresh */
    return [];
  }
}

/** Persist the manual profiles array. Silent on quota errors
 *  (localStorage typically caps at ~5MB; an Outbound is ~500B so
 *  10 000 profiles still fit comfortably). */
export function saveManualProfiles(profiles: Outbound[]): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(profiles));
  } catch {
    /* quota exceeded or storage disabled — non-fatal */
  }
}
