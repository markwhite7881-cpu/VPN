// Online GeoIP fallback for servers we can't infer a country for
// from their tag or hostname.
//
// Flow:
//   1. The caller passes us the list of profiles currently in
//      the picker. We extract the IPv4 addresses whose tag/TLD
//      still resolves to "??" (i.e. the cheap heuristic in
//      `flagForProfile` failed).
//   2. We load a `ip → countryCode` map from `localStorage`
//      (`singbox.geoip.v1`) — anything we've already resolved is
//      free and instant.
//   3. For the remaining IPs, we fire a single batch request via
//      the Tauri `lookup_geoip` command (ip-api.com). Up to 100
//      IPs per call, ~6 s timeout. We merge the response into the
//      map and persist it.
//   4. The next render uses the augmented map. Components decide
//      how to use it (e.g. `flagForProfile` takes it as an
//      optional `geoipByIp` argument).
//
// The cache is *permanent* per install: an IP that we resolved
// once never goes back to the network. If you want to flush it
// (e.g. you changed VPSes and reused the IP), clear that key in
// DevTools.

import { useEffect, useRef, useState } from "react";
import { api } from "@/lib/api";
import { isSupported } from "@/lib/outbound";
import { flagForProfile } from "@/lib/flags";
import type { Outbound } from "@/lib/types";

const CACHE_KEY = "singbox.geoip.v1";

function loadCache(): Record<string, string> {
  if (typeof window === "undefined") return {};
  try {
    const raw = window.localStorage.getItem(CACHE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed === "object") return parsed as Record<string, string>;
  } catch {
    /* corrupted cache — start fresh */
  }
  return {};
}

function saveCache(map: Record<string, string>): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(CACHE_KEY, JSON.stringify(map));
  } catch {
    /* quota — silently drop, we'll just re-ask next time */
  }
}

function isPublicIpv4(s: string): boolean {
  // Very small validator: four dotted octets, 0-255, no leading
  // zeros. We don't try to filter out RFC1918 etc. — ip-api is
  // happy to answer "this 10.x is in the US" or whatever, and
  // private ranges just get a 200 from a corporate geo DB which
  // is fine.
  if (!/^\d{1,3}(\.\d{1,3}){3}$/.test(s)) return false;
  for (const part of s.split(".")) {
    const n = Number(part);
    if (n < 0 || n > 255) return false;
  }
  return true;
}

export interface GeoIpState {
  /** ip → 2-letter country code. Empty while loading or offline. */
  byIp: Record<string, string>;
  /** True while a remote lookup is in flight. */
  loading: boolean;
}

export function useGeoIp(profiles: Outbound[]): GeoIpState {
  const [byIp, setByIp] = useState<Record<string, string>>(() => loadCache());
  const [loading, setLoading] = useState(false);
  // We use the profiles as a dependency token (length + a stable
  // signature) so the effect re-runs only when the set of
  // candidates really changes — the actual probe only fires when
  // there are IPs we haven't seen before.
  const sigRef = useRef("");
  const profilesRef = useRef(profiles);
  useEffect(() => {
    profilesRef.current = profiles;
  }, [profiles]);

  useEffect(() => {
    // Build a stable signature: sorted list of "host:tag" so we
    // only re-probe when the set of (host, tag) actually shifts.
    const supported = profiles.filter(isSupported);
    const sig = supported
      .map((p) => `${p.server}:${p.tag}`)
      .sort()
      .join("|");
    if (sig === sigRef.current) return;
    sigRef.current = sig;

    // 1. Find hosts the cheap heuristic couldn't resolve.
    const needLookup: string[] = [];
    for (const p of supported) {
      if (!isPublicIpv4(p.server)) continue;
      const cached = (() => {
        try {
          return loadCache()[p.server];
        } catch {
          return undefined;
        }
      })();
      if (cached) continue;
      const heuristic = flagForProfile({
        tag: p.tag,
        server: p.server,
      });
      if (heuristic.code === "??" && !needLookup.includes(p.server)) {
        needLookup.push(p.server);
      }
    }
    if (needLookup.length === 0) return;

    // 2. Fire one batch request.
    let cancelled = false;
    setLoading(true);
    void (async () => {
      try {
        const result = await api.lookupGeoip(needLookup);
        if (cancelled) return;
        setByIp((prev) => {
          const next = { ...prev };
          for (const [ip, code] of result) next[ip] = code;
          saveCache(next);
          return next;
        });
      } catch {
        // Network or ip-api 4xx — just keep whatever we already
        // have. The user can re-try next time a profile gets
        // added.
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [profiles]);

  return { byIp, loading };
}
