import type { EngineKind } from "./types";

export function shouldFrontendApplySystemProxy(
  engine: EngineKind | null | undefined,
  tunnelMode: "system_proxy" | "both" | "tun" | "none",
): boolean {
  return engine !== "xray" && (tunnelMode === "system_proxy" || tunnelMode === "both");
}
