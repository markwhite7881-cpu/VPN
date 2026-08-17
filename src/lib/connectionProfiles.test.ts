import { describe, expect, it, vi } from "vitest";
import type { Outbound, SubscriptionSnapshot } from "./types";
import {
  buildConnectionProfiles,
  LEGACY_SUBSCRIPTIONS_KEY,
  managedSelectionForProfile,
  migrateLegacySubscriptions,
} from "./connectionProfiles";

const manual: Outbound[] = [
  { protocol: "unsupported", raw: "manual-a", reason: "test" },
  { protocol: "unsupported", raw: "manual-b", reason: "test" },
];

const snapshot: SubscriptionSnapshot = {
  subscriptions: [],
  link_outbounds: [
    {
      subscription_id: "sub-a",
      links: [
        { key: "index-0", label: "vless link 1", protocol: "vless" },
        { key: "index-1", label: "trojan link 2", protocol: "trojan" },
      ],
    },
  ],
};

describe("connection profile selection", () => {
  it("keeps manual profiles first and represents subscriptions as opaque refs", () => {
    const profiles = buildConnectionProfiles(manual, snapshot);

    expect(profiles.slice(0, 2)).toEqual([
      { kind: "manual", outbound: manual[0] },
      { kind: "manual", outbound: manual[1] },
    ]);
    expect(profiles.slice(2)).toEqual([
      {
        kind: "subscription",
        reference: { subscription_id: "sub-a", link_key: "index-0" },
        label: "vless link 1",
        protocol: "vless",
      },
      {
        kind: "subscription",
        reference: { subscription_id: "sub-a", link_key: "index-1" },
        label: "trojan link 2",
        protocol: "trojan",
      },
    ]);
  });

  it("uses all links for Auto and one opaque reference for an explicit subscription", () => {
    const profiles = buildConnectionProfiles(manual, snapshot);

    expect(managedSelectionForProfile(manual, profiles, -1)).toEqual({
      manualOutbounds: manual,
      selectAllSubscriptionLinks: true,
    });
    expect(managedSelectionForProfile(manual, profiles, 3)).toEqual({
      manualOutbounds: manual,
      subscriptionLinks: [{ subscription_id: "sub-a", link_key: "index-1" }],
      selectAllSubscriptionLinks: false,
    });
  });
});

describe("legacy subscription migration", () => {
  it("keeps storage when backend migration fails", async () => {
    const storage = { removeItem: vi.fn() } as unknown as Storage;
    await expect(migrateLegacySubscriptions(["legacy"], async () => { throw new Error("failed"); }, storage)).rejects.toThrow("failed");
    expect(storage.removeItem).not.toHaveBeenCalled();
  });

  it("removes storage only after backend migration succeeds", async () => {
    const storage = { removeItem: vi.fn() } as unknown as Storage;
    await expect(migrateLegacySubscriptions(["legacy"], async () => "ok", storage)).resolves.toBe("ok");
    expect(storage.removeItem).toHaveBeenCalledWith(LEGACY_SUBSCRIPTIONS_KEY);
  });
});
