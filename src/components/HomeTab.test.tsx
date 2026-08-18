import { describe, expect, it } from "vitest";
import type { ConnectionProfile } from "@/lib/connectionProfiles";

import { connectionProfileDisplay, powerButtonClasses } from "./HomeTab";

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

  it("uses green connected classes only while running", () => {
    expect(powerButtonClasses("running")).toContain("bg-success");
    expect(powerButtonClasses("starting")).not.toContain("bg-success");
    expect(powerButtonClasses("stopping")).not.toContain("bg-success");
  });
});
