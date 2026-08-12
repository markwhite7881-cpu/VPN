// Per-server latency probe.
//
// We use the Tauri `ping_endpoint` command (raw `TcpStream::connect`
// with timeout) instead of sing-box's `GET /proxies/{name}/delay`.
// Reasons:
//   1. It works *while the tunnel is down* — the user can see the
//      best server before they connect and pick it from the picker
//      without having to start the tunnel first to "warm up" the
//      latency list.
//   2. It's sing-box-version-agnostic — the upstream clash API
//      was changed / moved in some forks, but a TCP connect is a
//      TCP connect.
//   3. The numbers correlate well with TLS handshake latency to
//      the same host, which is what actually drives proxy speed.
//
// The probe is parallel (one shot per server, ~2s timeout) and
// re-runs every 10 s so the values stay current as the network
// drifts.

import { useEffect, useRef, useState } from "react";
import { api } from "@/lib/api";
import { isSupported } from "@/lib/outbound";
import type { Outbound } from "@/lib/types";

/** ms → 0..4 signal-bars bucket. */
export function latencyToBars(ms: number | null | undefined): number {
  if (ms == null || !Number.isFinite(ms) || ms < 0) return 0;
  if (ms < 50) return 4;
  if (ms < 100) return 3;
  if (ms < 200) return 2;
  if (ms < 500) return 1;
  return 0;
}

export interface LatencyState {
  /** tag → ms. Missing means "no data yet" or "probe failed". */
  byTag: Map<string, number>;
  /** True while the initial probe pass is in flight. */
  loading: boolean;
}

const PROBE_INTERVAL_MS = 10_000;
const PROBE_TIMEOUT_MS = 2_000;

export function useServerLatency(
  profiles: Outbound[],
  // Kept in the signature for backward-compat (the Home tab still
  // passes the running flag), but we no longer use it — the probe
  // runs unconditionally so the picker is useful before connect.
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  _isRunning: boolean = true,
): LatencyState {
  const [byTag, setByTag] = useState<Map<string, number>>(() => new Map());
  const [loading, setLoading] = useState(false);
  // Hold the latest profiles in a ref so the probe loop can
  // re-evaluate without us having to tear down and restart the
  // interval on every state change.
  const profilesRef = useRef(profiles);
  useEffect(() => {
    profilesRef.current = profiles;
  }, [profiles]);

  useEffect(() => {
    let cancelled = false;
    let timer: number | null = null;

    const probe = async () => {
      const supported = profilesRef.current.filter(isSupported);
      if (supported.length === 0) return;
      setLoading(true);
      const next = new Map<string, number>();
      // Probe in parallel — each ping is at most `PROBE_TIMEOUT_MS`,
      // so doing them serially would be 2s × N. With ~20 servers
      // the parallel pass finishes in ~2s flat.
      const results = await Promise.allSettled(
        supported.map((p) =>
          api
            .pingEndpoint(p.server, p.port, PROBE_TIMEOUT_MS)
            .then((d) => ({ tag: p.tag, ms: d }))
            .catch(() => ({ tag: p.tag, ms: null as number | null })),
        ),
      );
      if (cancelled) return;
      for (const r of results) {
        if (r.status === "fulfilled" && r.value.ms != null) {
          next.set(r.value.tag, r.value.ms);
        }
      }
      setByTag(next);
      setLoading(false);
    };

    // Kick off immediately and then every 10s.
    void probe();
    timer = window.setInterval(() => void probe(), PROBE_INTERVAL_MS);
    return () => {
      cancelled = true;
      if (timer != null) window.clearInterval(timer);
    };
  }, [profiles.length]);

  return { byTag, loading };
}
