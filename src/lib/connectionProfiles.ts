import type {
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
    };

export type ManagedConnectionSelection = {
  manualOutbounds: Outbound[];
  subscriptionLinks?: SubscriptionLinkRef[];
  selectAllSubscriptionLinks: boolean;
};

/** Manual entries always precede backend-owned opaque subscription links. */
export function buildConnectionProfiles(
  manualOutbounds: Outbound[],
  snapshot: SubscriptionSnapshot,
): ConnectionProfile[] {
  return [
    ...manualOutbounds.map((outbound) => ({ kind: "manual" as const, outbound })),
    ...snapshot.link_outbounds.flatMap((group) =>
      group.links.map((link) => ({
        kind: "subscription" as const,
        reference: { subscription_id: group.subscription_id, link_key: link.key },
        label: link.label,
        protocol: link.protocol,
      })),
    ),
  ];
}

/**
 * Auto includes every backend link. An explicit subscription choice passes
 * only its opaque reference; React never reconstructs an Outbound from it.
 */
export function managedSelectionForProfile(
  manualOutbounds: Outbound[],
  profiles: ConnectionProfile[],
  selectedIndex: number,
): ManagedConnectionSelection {
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
