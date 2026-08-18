import { describe, expect, it } from "vitest";
import { proxiesCapability } from "./ProxiesCard";
import type { StatusReport } from "@/lib/types";

const runningXray: StatusReport = {
  status: "running",
  pid: 100,
  uptime_secs: 1,
  last_exit_code: null,
  last_error: null,
  engine: "xray",
  profile_key: "profile-1",
  profile_name: "Germany",
};

describe("proxiesCapability", () => {
  it("blocks Clash proxy controls for a running Xray connection", () => {
    expect(proxiesCapability(runningXray)).toBe("xray_unsupported");
  });
});
