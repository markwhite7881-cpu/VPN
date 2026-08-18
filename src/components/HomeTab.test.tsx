import { describe, expect, it } from "vitest";
import type { ConnectionProfile } from "@/lib/connectionProfiles";

import { connectionProfileDisplay, groupHomeProfiles, powerButtonClasses, subscriptionGroupLabel } from "./HomeTab";

describe("HomeTab presentation helpers", () => {
  const readyXrayProfile: ConnectionProfile = {
    kind: "ready_config",
    subscriptionId: "subscription-1",
    key: "profile-1",
    name: "Germany",
    engine: "xray",
  };

  it("uses safe Xray metadata for a ready profile", () => {
    expect(
      connectionProfileDisplay(
        readyXrayProfile,
        new Map([["subscription-1:profile-1", { country_code: "DE", latency_ms: 47 }]]),
        {},
        new Map(),
      ),
    ).toMatchObject({ code: "DE", ms: 47 });
  });

  it("uses an honest fallback when Xray metadata is unavailable", () => {
    expect(connectionProfileDisplay(readyXrayProfile, new Map(), {}, new Map())).toMatchObject({
      code: "??",
      ms: undefined,
    });
  });

  it("preserves manual flag and latency behavior", () => {
    const manualProfile: ConnectionProfile = {
      kind: "manual",
      outbound: {
        protocol: "vless",
        tag: "Germany",
        server: "de.example.test",
        port: 443,
        uuid: "test-uuid",
        flow: undefined,
        transport: { kind: "tcp" },
        tls: { enabled: false, alpn: [], allow_insecure: false },
      },
    };

    expect(connectionProfileDisplay(manualProfile, new Map(), {}, new Map([["Germany", 23]]))).toMatchObject({
      code: "DE",
      ms: 23,
    });
  });

  it("uses safe subscription summary names for grouped labels", () => {
    expect(subscriptionGroupLabel("subscription-1", new Map([["subscription-1", "Work subscription"]]))).toBe("Work subscription");
    expect(subscriptionGroupLabel("missing", new Map())).toBe("Subscription");
  });

  it("groups Home profiles without changing flat selection indexes", () => {
    const manualProfile: ConnectionProfile = {
      kind: "manual",
      outbound: {
        protocol: "vless",
        tag: "Manual DE",
        server: "de.example.test",
        port: 443,
        uuid: "test-uuid",
        flow: undefined,
        transport: { kind: "tcp" },
        tls: { enabled: false, alpn: [], allow_insecure: false },
      },
    };
    const opaqueSubscription: ConnectionProfile = {
      kind: "subscription",
      reference: { subscription_id: "sub-a", link_key: "link-1" },
      label: "Opaque NL",
      protocol: "vless",
    };
    const readySingboxProfile: ConnectionProfile = {
      kind: "ready_config",
      subscriptionId: "sub-a",
      key: "sb-1",
      name: "France",
      engine: "singbox",
    };
    const readyXrayProfile: ConnectionProfile = {
      kind: "ready_config",
      subscriptionId: "sub-b",
      key: "xray-1",
      name: "Finland",
      engine: "xray",
    };
    const profiles = [manualProfile, opaqueSubscription, readySingboxProfile, readyXrayProfile];

    expect(groupHomeProfiles(profiles)).toEqual({
      manual: [{ index: 0, profile: manualProfile }],
      subscriptions: [
        {
          id: "sub-a",
          rows: [
            { index: 1, profile: opaqueSubscription },
            { index: 2, profile: readySingboxProfile },
          ],
        },
        { id: "sub-b", rows: [{ index: 3, profile: readyXrayProfile }] },
      ],
    });
  });

  it("uses green connected classes only while running", () => {
    expect(powerButtonClasses("running")).toContain("bg-success");
    expect(powerButtonClasses("starting")).not.toContain("bg-success");
    expect(powerButtonClasses("stopping")).not.toContain("bg-success");
  });
});
