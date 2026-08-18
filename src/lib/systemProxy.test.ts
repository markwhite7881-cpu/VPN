import { describe, expect, it } from "vitest";
import { shouldFrontendApplySystemProxy } from "./systemProxy";

describe("shouldFrontendApplySystemProxy", () => {
  it("leaves the Xray-managed proxy endpoint untouched", () => {
    expect(shouldFrontendApplySystemProxy("xray", "system_proxy")).toBe(false);
  });

  it("keeps the existing sing-box system-proxy path", () => {
    expect(shouldFrontendApplySystemProxy("singbox", "system_proxy")).toBe(true);
  });
});
