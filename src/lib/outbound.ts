// Convenience accessors for the discriminated `Outbound` union.
// The Rust side models every share-link with a `tag` + `server` field;
// the only variant that lacks them is `unsupported`. These helpers
// keep the type narrowing at the call site to a single `in` check.

import type { Outbound } from "./types";

type Supported = Exclude<Outbound, { protocol: "unsupported" }>;

/** Type guard: outbound is a real share-link (not "unsupported"). */
export function isSupported(o: Outbound): o is Supported {
  return o.protocol !== "unsupported";
}

/** Display label for a profile (tag if present, else server:port). */
export function profileLabel(o: Outbound): string {
  if (o.protocol === "unsupported") return "Unsupported link";
  return o.tag || `${o.server}:${o.port}`;
}

/** Server:port string for a profile, or "" for unsupported. */
export function profileEndpoint(o: Outbound): string {
  if (o.protocol === "unsupported") return "";
  return `${o.server}:${o.port}`;
}
