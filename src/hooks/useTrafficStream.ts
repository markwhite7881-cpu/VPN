import { useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api } from "@/lib/api";
import type { TrafficSample } from "@/lib/types";

const inTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const SAMPLE_LIMIT = 60; // 60 seconds at 1 Hz

export interface UseTrafficStream {
  samples: TrafficSample[];
  current: TrafficSample | null;
}

/**
 * Hook: subscribes to the `traffic` Tauri event and keeps a rolling
 * window of the most recent samples. In browser preview mode it
 * synthesises a low-volume feed so the chart is still demoable.
 */
export function useTrafficStream(
  enabled: boolean,
  profileCount: number,
): UseTrafficStream {
  const [samples, setSamples] = useState<TrafficSample[]>([]);
  const [current, setCurrent] = useState<TrafficSample | null>(null);
  const tickRef = useRef<number | null>(null);

  useEffect(() => {
    if (!enabled) {
      setSamples([]);
      setCurrent(null);
      return;
    }

    if (!inTauri) {
      // Browser preview: synthesise a slow-moving traffic curve so
      // the chart isn't a flat line. We modulate the down/up speeds
      // with a sine wave plus the profile count (more profiles → more
      // variation) so the demo looks alive.
      let frame = 0;
      const fake = () => {
        frame += 1;
        const t = frame / 6;
        const baseDown = 800_000 + 1_200_000 * Math.abs(Math.sin(t));
        const baseUp = 60_000 + 90_000 * Math.abs(Math.sin(t * 0.7 + 0.4));
        const wiggleDown = (Math.random() - 0.5) * 200_000;
        const wiggleUp = (Math.random() - 0.5) * 20_000;
        const s: TrafficSample = {
          up_bps: Math.max(0, Math.round(baseUp + wiggleUp)),
          down_bps: Math.max(0, Math.round(baseDown + wiggleDown)),
          up_total: 0,
          down_total: 0,
          ts_ms: Date.now(),
        };
        setSamples((prev) => {
          const next = prev.length >= SAMPLE_LIMIT ? prev.slice(1) : prev.slice();
          next.push(s);
          return next;
        });
        setCurrent(s);
      };
      // Seed with a flat baseline so the chart doesn't start empty.
      const seed: TrafficSample[] = Array.from({ length: 20 }, (_, i) => ({
        up_bps: 0,
        down_bps: 0,
        up_total: 0,
        down_total: 0,
        ts_ms: Date.now() - (20 - i) * 1000,
      }));
      setSamples(seed);
      fake();
      tickRef.current = window.setInterval(fake, 1000);
      return () => {
        if (tickRef.current != null) {
          window.clearInterval(tickRef.current);
          tickRef.current = null;
        }
      };
    }

    // Real Tauri shell: subscribe to the `traffic` event.
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;

    (async () => {
      try {
        const u = await listen<TrafficSample>("traffic", (e) => {
          if (cancelled) return;
          const s = e.payload;
          setSamples((prev) => {
            const next = prev.length >= SAMPLE_LIMIT ? prev.slice(1) : prev.slice();
            next.push(s);
            return next;
          });
          setCurrent(s);
        });
        if (cancelled) {
          u();
          return;
        }
        unlisten = u;
      } catch (err) {
        // eslint-disable-next-line no-console
        console.warn("traffic listen failed", err);
      }
    })();

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [enabled, profileCount]);

  return { samples, current };
}
