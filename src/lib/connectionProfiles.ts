import type {
  EngineKind,
  Outbound,
  SubscriptionLinkRef,
  SubscriptionSnapshot,
} from "./types";

export const LEGACY_SUBSCRIPTIONS_KEY = "singbox-client.subscriptions.v1";

export type ConnectionProfile =
  | { kind: "manual"; outbound: Outbound }
  | {
      kind: "subscription";
      reference: SubscriptionLinkRef;
      label: string;
      protocol: string;
    }
  | {
      kind: "ready_config";
      subscriptionId: string;
      key: string;
      name: string;
      engine: EngineKind;
    };

export type ManagedConnectionSelection = {
  manualOutbounds: Outbound[];
  subscriptionLinks?: SubscriptionLinkRef[];
  selectAllSubscriptionLinks: boolean;
  profile?: { subscription_id: string; child_key: string };
};

/**
 * Manual entries come first. Subscription-owned entries then preserve their
 * backend snapshot order: opaque links before safe ready-config children.
 */
export function buildConnectionProfiles(
  manualOutbounds: Outbound[],
  snapshot: SubscriptionSnapshot,
): ConnectionProfile[] {
  const profiles: ConnectionProfile[] = manualOutbounds.map((outbound) => ({
    kind: "manual",
    outbound,
  }));
  const linksBySubscription = new Map(
    snapshot.link_outbounds.map((group) => [group.subscription_id, group]),
  );
  const seenSubscriptions = new Set<string>();

  for (const subscription of snapshot.subscriptions) {
    seenSubscriptions.add(subscription.id);
    const linkGroup = linksBySubscription.get(subscription.id);
    if (linkGroup) {
      profiles.push(
        ...linkGroup.links.map((link) => ({
          kind: "subscription" as const,
          reference: { subscription_id: linkGroup.subscription_id, link_key: link.key },
          label: link.label,
          protocol: link.protocol,
        })),
      );
    }
    profiles.push(
      ...subscription.children.map((child) => ({
        kind: "ready_config" as const,
        subscriptionId: subscription.id,
        key: child.key,
        name: child.name,
        engine: child.engine,
      })),
    );
  }

  // Preserve existing snapshots that contain a link group without a summary.
  for (const linkGroup of snapshot.link_outbounds) {
    if (seenSubscriptions.has(linkGroup.subscription_id)) continue;
    profiles.push(
      ...linkGroup.links.map((link) => ({
        kind: "subscription" as const,
        reference: { subscription_id: linkGroup.subscription_id, link_key: link.key },
        label: link.label,
        protocol: link.protocol,
      })),
    );
  }
  return profiles;
}

/** Ready configurations are executable through their backend-owned engine path. */
export function canStartManagedSelection(
  profiles: ConnectionProfile[],
  selectedIndex: number,
): boolean {
  if (selectedIndex === -1) {
    return profiles.some((profile) => profile.kind !== "ready_config");
  }
  const selected = profiles[selectedIndex];
  return !!selected;
}

/**
 * Auto includes every backend link. An explicit subscription choice passes
 * only its opaque reference; React never reconstructs an Outbound from it.
 */
export function managedSelectionForProfile(
  manualOutbounds: Outbound[],
  profiles: ConnectionProfile[],
  selectedIndex: number,
): ManagedConnectionSelection | null {
  const selected = profiles[selectedIndex];
  if (selectedIndex === -1 || !selected) {
    return {
      manualOutbounds,
      selectAllSubscriptionLinks: true,
    };
  }
  if (selected.kind === "subscription") {
    return {
      manualOutbounds,
      subscriptionLinks: [selected.reference],
      selectAllSubscriptionLinks: false,
    };
  }
  if (selected.kind === "ready_config") {
    return {
      manualOutbounds,
      selectAllSubscriptionLinks: false,
      profile: { subscription_id: selected.subscriptionId, child_key: selected.key },
    };
  }
  return {
    manualOutbounds,
    selectAllSubscriptionLinks: false,
  };
}

export function selectedManualOutbound(
  profiles: ConnectionProfile[],
  selectedIndex: number,
): Outbound | null {
  const selected = profiles[selectedIndex];
  return selected?.kind === "manual" ? selected.outbound : null;
}

export async function migrateLegacySubscriptions<T>(
  entries: unknown[],
  migrate: (entries: unknown[]) => Promise<T>,
  storage: Pick<Storage, "removeItem">,
): Promise<T | null> {
  if (!entries.length) return null;
  const result = await migrate(entries);
  storage.removeItem(LEGACY_SUBSCRIPTIONS_KEY);
  return result;
}
