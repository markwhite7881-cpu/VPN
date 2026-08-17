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

  it("passes ready-config children through the backend-owned engine path", () => {
    const profiles = buildConnectionProfiles(manual, {
      subscriptions: [
        {
          id: "sub-a", name: "Links", kind: "link_list", engine: "singbox", interval_minutes: 60,
          active_child_key: null, children: [], metadata: {}, last_success_at: null, last_http_status: 200, last_error: null,
        },
        {
          id: "sub-b", name: "Bundle", kind: "xray_bundle", engine: "xray", interval_minutes: 60,
          active_child_key: "index-0",
          children: [{ key: "index-0", name: "Primary", engine: "xray" }],
          metadata: {}, last_success_at: null, last_http_status: 200, last_error: null,
        },
      ],
      link_outbounds: snapshot.link_outbounds,
    });

    expect(profiles.slice(2)).toEqual([
      { kind: "subscription", reference: { subscription_id: "sub-a", link_key: "index-0" }, label: "vless link 1", protocol: "vless" },
      { kind: "subscription", reference: { subscription_id: "sub-a", link_key: "index-1" }, label: "trojan link 2", protocol: "trojan" },
      { kind: "ready_config", subscriptionId: "sub-b", key: "index-0", name: "Primary", engine: "xray" },
    ]);
    expect(managedSelectionForProfile(manual, profiles, 4)).toEqual({
      manualOutbounds: manual,
      selectAllSubscriptionLinks: false,
      profile: { subscription_id: "sub-b", child_key: "index-0" },
    });
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
