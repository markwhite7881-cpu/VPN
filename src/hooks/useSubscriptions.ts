import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "@/lib/api";
import type { SubscriptionSnapshot, SubscriptionSummary } from "@/lib/types";

const STORAGE_KEY = "singbox-client.subscriptions.v1";

type LegacySubscription = { id: string; name: string; url: string; intervalMinutes: number };

function readLegacy(): LegacySubscription[] {
  if (typeof window === "undefined") return [];
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter((item) => item && typeof item.id === "string" && typeof item.name === "string" && typeof item.url === "string") : [];
  } catch { return []; }
}

export function useSubscriptions() {
  const [snapshot, setSnapshot] = useState<SubscriptionSnapshot>({ subscriptions: [], link_outbounds: [] });
  const [fetching, setFetching] = useState<Record<string, boolean>>({});
  const tickRef = useRef<number | null>(null);

  const load = useCallback(async () => {
    const legacy = readLegacy();
    if (legacy.length) {
      await api.migrateLegacySubscriptions(legacy);
      window.localStorage.removeItem(STORAGE_KEY);
    }
    setSnapshot(await api.listSubscriptions());
  }, []);

  useEffect(() => { void load().catch(() => undefined); }, [load]);

  const refreshOne = useCallback(async (id: string) => {
    setFetching((prev) => ({ ...prev, [id]: true }));
    try {
      await api.refreshSubscription(id);
      setSnapshot(await api.listSubscriptions());
    } finally {
      setFetching((prev) => ({ ...prev, [id]: false }));
    }
  }, []);
  const refreshAll = useCallback(async () => {
    await Promise.all(snapshot.subscriptions.map((s) => refreshOne(s.id)));
  }, [refreshOne, snapshot.subscriptions]);
  const add = useCallback(async (input: { name?: string; url: string; intervalMinutes?: number }) => {
    await api.addSubscription({ name: input.name?.trim() || "Subscription", url: input.url.trim(), intervalMinutes: input.intervalMinutes ?? 60 });
    await load();
  }, [load]);
  const remove = useCallback(async (id: string) => { await api.removeSubscription(id); await load(); }, [load]);
  const setIntervalFor = useCallback(async (id: string, minutes: number) => { await api.setSubscriptionInterval(id, minutes); await load(); }, [load]);
  const selectChild = useCallback(async (id: string, childKey: string) => { await api.selectSubscriptionChild(id, childKey); await load(); }, [load]);

  useEffect(() => {
    tickRef.current = window.setInterval(() => {
      const now = Date.now();
      snapshot.subscriptions.forEach((s) => {
        if (s.interval_minutes > 0 && (!s.last_success_at || now - Date.parse(s.last_success_at) >= s.interval_minutes * 60_000)) void refreshOne(s.id);
      });
    }, 30_000);
    return () => { if (tickRef.current != null) window.clearInterval(tickRef.current); };
  }, [refreshOne, snapshot.subscriptions]);

  return {
    subs: snapshot.subscriptions,
    snapshot,
    fetching,
    add,
    remove,
    refreshOne,
    refreshAll,
    setIntervalFor,
    selectChild,
    getHwid: api.getSubscriptionHwid,
    resetHwid: api.resetSubscriptionHwid,
  };
}

export type { SubscriptionSummary };
