import { useEffect, useRef, useState } from "react";
import { api } from "@/lib/api";
import type { ConnectionProfile } from "@/lib/connectionProfiles";
import type { HomeProfileMetadata } from "@/lib/types";

export interface ReadyXrayProfileKey {
  subscriptionId: string;
  childKey: string;
  cacheKey: string;
}

export function readyXrayProfileKeys(profiles: ConnectionProfile[]): ReadyXrayProfileKey[] {
  return profiles
    .filter(
      (profile): profile is Extract<ConnectionProfile, { kind: "ready_config" }> =>
        profile.kind === "ready_config" && profile.engine === "xray",
    )
    .map(({ subscriptionId, key }) => ({
      subscriptionId,
      childKey: key,
      cacheKey: `${subscriptionId}:${key}`,
    }));
}

export function mergeReadyProfileMetadata(
  previous: ReadonlyMap<string, HomeProfileMetadata>,
  keys: ReadyXrayProfileKey[],
  resolved: ReadonlyMap<string, HomeProfileMetadata>,
): Map<string, HomeProfileMetadata> {
  const current = new Set(keys.map(({ cacheKey }) => cacheKey));
  const next = new Map<string, HomeProfileMetadata>();
  for (const [cacheKey, metadata] of previous) {
    if (current.has(cacheKey)) next.set(cacheKey, metadata);
  }
  for (const [cacheKey, metadata] of resolved) {
    if (current.has(cacheKey)) next.set(cacheKey, metadata);
  }
  return next;
}

function immutableMap<K, V>(source: Map<K, V>): ReadonlyMap<K, V> {
  return source;
}

export function useReadyProfileMetadata(
  profiles: ConnectionProfile[],
): ReadonlyMap<string, HomeProfileMetadata> {
  const [metadata, setMetadata] = useState<Map<string, HomeProfileMetadata>>(() => new Map());
  const inFlight = useRef(new Map<string, Promise<HomeProfileMetadata>>());
  const keys = readyXrayProfileKeys(profiles);
  const signature = keys.map(({ cacheKey }) => cacheKey).join("|");

  useEffect(() => {
    let cancelled = false;
    const activeKeys = new Set(keys.map(({ cacheKey }) => cacheKey));
    setMetadata((previous) => {
      const next = new Map<string, HomeProfileMetadata>();
      for (const [cacheKey, value] of previous) {
        if (activeKeys.has(cacheKey)) next.set(cacheKey, value);
      }
      return next;
    });

    const pending = new Map<string, Promise<HomeProfileMetadata>>();
    for (const key of keys) {
      if (metadata.has(key.cacheKey)) continue;
      const existing = inFlight.current.get(key.cacheKey);
      const request = existing ?? api.getReadyProfileMetadata(key.subscriptionId, key.childKey);
      if (!existing) inFlight.current.set(key.cacheKey, request);
      pending.set(key.cacheKey, request);
      void request
        .then((value) => {
          if (cancelled) return;
          setMetadata((previous) => {
            const next = new Map(previous);
            next.set(key.cacheKey, value);
            return next;
          });
        })
        .catch(() => undefined)
        .finally(() => {
          if (inFlight.current.get(key.cacheKey) === request) inFlight.current.delete(key.cacheKey);
        });
    }
    void pending;
    return () => {
      cancelled = true;
    };
    // The signature is the intentional dependency token; metadata changes do
    // not restart successful or in-flight probes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [signature]);

  return immutableMap(metadata);
}
