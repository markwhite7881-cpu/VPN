import { describe, expect, it } from "vitest";
import type { ConnectionProfile } from "@/lib/connectionProfiles";

import {
  readyXrayProfileKeys,
  mergeReadyProfileMetadata,
} from "./useReadyProfileMetadata";

describe("useReadyProfileMetadata", () => {
  const profiles: ConnectionProfile[] = [
    { kind: "ready_config", subscriptionId: "sub-1", key: "xray-1", name: "DE", engine: "xray" },
    { kind: "ready_config", subscriptionId: "sub-2", key: "sb-1", name: "SB", engine: "singbox" },
    { kind: "subscription", reference: { subscription_id: "sub-3", link_key: "link-1" }, label: "link", protocol: "vless" },
  ];

  it("selects only ready Xray profiles using opaque identifiers", () => {
    expect(readyXrayProfileKeys(profiles)).toEqual([
      { subscriptionId: "sub-1", childKey: "xray-1", cacheKey: "sub-1:xray-1" },
    ]);
  });

  it("preserves successful cache entries after failure and removes absent profiles", () => {
    const previous = new Map([
      ["sub-1:xray-1", { country_code: "DE", latency_ms: 47 }],
      ["sub-old:gone", { country_code: "US", latency_ms: 12 }],
    ]);
    const next = mergeReadyProfileMetadata(
      previous,
      [{ subscriptionId: "sub-1", childKey: "xray-1", cacheKey: "sub-1:xray-1" }],
      new Map(),
    );
    expect([...next.entries()]).toEqual([
      ["sub-1:xray-1", { country_code: "DE", latency_ms: 47 }],
    ]);
    expect(next).not.toBe(previous);
  });
});
