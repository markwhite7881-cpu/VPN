import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "@/lib/api";
import type { Outbound, Subscription } from "@/lib/types";

const STORAGE_KEY = "singbox-client.subscriptions.v1";

function loadFromStorage(): Subscription[] {
  if (typeof window === "undefined") return [];
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (s) =>
        s &&
        typeof s.id === "string" &&
        typeof s.name === "string" &&
        typeof s.url === "string",
    ) as Subscription[];
  } catch {
    return [];
  }
}

function saveToStorage(subs: Subscription[]) {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(subs));
  } catch {
    /* quota exceeded; ignore */
  }
}

function makeId(): string {
  // crypto.randomUUID is widely available in modern browsers and Tauri
  // webviews. Fall back to a timestamp-based id if it isn't.
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `sub-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

export interface FetchResult {
  outbounds: Outbound[];
  errors: number;
}

/**
 * Hook: persists a list of subscriptions to localStorage, fetches
 * them, and returns the merged outbounds. Auto-refresh is driven by
 * per-subscription `intervalMinutes`.
 */
export function useSubscriptions() {
  const [subs, setSubs] = useState<Subscription[]>(() => loadFromStorage());
  const [fetching, setFetching] = useState<Record<string, boolean>>({});
  const [lastResult, setLastResult] = useState<Record<string, FetchResult>>(
    {},
  );
  const tickRef = useRef<number | null>(null);

  // Persist on every change.
  useEffect(() => {
    saveToStorage(subs);
  }, [subs]);

  const refreshOne = useCallback(
    async (id: string) => {
      const sub = subs.find((s) => s.id === id);
      if (!sub) return;
      setFetching((prev) => ({ ...prev, [id]: true }));
      try {
        const result = await api.fetchSubscription(sub.url);
        const errs = result.failures.length;
        setLastResult((prev) => ({
          ...prev,
          [id]: { outbounds: result.outbounds, errors: errs },
        }));
        setSubs((prev) =>
          prev.map((s) =>
            s.id === id
              ? {
                  ...s,
                  lastFetchedAt: new Date().toISOString(),
                  lastCount: result.outbounds.length,
                  lastError:
                    result.outbounds.length === 0
                      ? "0 profiles parsed"
                      : null,
                  lastErrorKind:
                    result.outbounds.length === 0 ? "empty" : null,
                }
              : s,
          ),
        );
      } catch (e) {
        const msg = (e as Error).message || String(e);
        setSubs((prev) =>
          prev.map((s) =>
            s.id === id
              ? { ...s, lastError: msg, lastErrorKind: "network" }
              : s,
          ),
        );
      } finally {
        setFetching((prev) => ({ ...prev, [id]: false }));
      }
    },
    [subs],
  );

  const refreshAll = useCallback(async () => {
    await Promise.all(subs.map((s) => refreshOne(s.id)));
  }, [subs, refreshOne]);

  // Auto-refresh tick. Runs every 30s; checks which subs are due.
  useEffect(() => {
    const interval = 30_000; // 30s tick
    const onTick = () => {
      const now = Date.now();
      subs.forEach((s) => {
        if (s.intervalMinutes <= 0) return;
        const lastTs = s.lastFetchedAt
          ? new Date(s.lastFetchedAt).getTime()
          : 0;
        const due = now - lastTs >= s.intervalMinutes * 60_000;
        if (due) {
          // Fire-and-forget; refreshOne updates state.
          refreshOne(s.id);
        }
      });
    };
    onTick(); // run once on mount
    tickRef.current = window.setInterval(onTick, interval);
    return () => {
      if (tickRef.current != null) {
        window.clearInterval(tickRef.current);
        tickRef.current = null;
      }
    };
  }, [subs, refreshOne]);

  // One-shot fetch on app start: subscription URLs are persisted,
  // but the parsed `lastResult` is React state and goes away on
  // every relaunch. The auto-refresh tick above only re-fetches
  // subs that are "due" (intervalMinutes since lastFetchedAt) — so
  // a sub fetched 5 minutes before launch won't be re-fetched, and
  // the server list stays empty until the interval passes.
  //
  // For a "fresh launch" UX we always kick off one fetch per sub
  // on mount, regardless of how recent the last fetch was. The
  // network call is cheap (a single GET) and the UI shows a
  // `fetching` indicator per sub so the user knows it's in flight.
  // If a user wants a true "offline mode" they can set
  // `intervalMinutes = 0` on the subscription.
  useEffect(() => {
    if (subs.length === 0) return;
    subs.forEach((s) => {
      // refreshOne updates state; we don't await — UI shows the
      // fetching indicator.
      void refreshOne(s.id);
    });
    // Intentionally depend only on `subs.length` (not the full
    // array): we want to fire once on first mount and not re-fire
    // when the auto-refresh tick mutates `subs` (e.g. updates
    // lastFetchedAt). A length change means a real add/remove.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [subs.length]);

  const add = useCallback(
    (input: { name?: string; url: string; intervalMinutes?: number }) => {
      const id = makeId();
      let displayName = input.name?.trim();
      if (!displayName) {
        try {
          displayName = new URL(input.url).hostname || "subscription";
        } catch {
          displayName = "subscription";
        }
      }
      const sub: Subscription = {
        id,
        name: displayName,
        url: input.url.trim(),
        intervalMinutes: input.intervalMinutes ?? 60,
        lastFetchedAt: null,
        lastCount: 0,
        lastError: null,
        lastErrorKind: null,
      };
      setSubs((prev) => [...prev, sub]);
      // Fire first fetch immediately.
      setTimeout(() => refreshOne(id), 0);
    },
    [refreshOne],
  );

  const remove = useCallback((id: string) => {
    setSubs((prev) => prev.filter((s) => s.id !== id));
  }, []);

  const setIntervalFor = useCallback((id: string, mins: number) => {
    setSubs((prev) =>
      prev.map((s) => (s.id === id ? { ...s, intervalMinutes: mins } : s)),
    );
  }, []);

  return {
    subs,
    add,
    remove,
    refreshOne,
    refreshAll,
    setIntervalFor,
    fetching,
    lastResult,
  };
}

/** Aggregate the latest outbounds across all subscriptions. */
export function mergeSubscriptionResults(
  subs: Subscription[],
  lastResult: Record<string, FetchResult>,
): Outbound[] {
  const out: Outbound[] = [];
  for (const s of subs) {
    const r = lastResult[s.id];
    if (r) out.push(...r.outbounds);
  }
  return out;
}
