// Preset rule-sets (Loyalsoldier v2ray-rules-dat + meta-rules-dat) and
// pre-baked CustomRule "starter" rules (Bypass LAN, Reject IPv6, etc).
//
// Source: https://github.com/Loyalsoldier/v2ray-rules-dat  (release branch)
//         https://github.com/Loyalsoldier/meta-rules-dat   (sing-box compatible mirror)
//
// All URLs resolve to .srs (binary, sing-box 1.10+ format). Last verified
// against Loyalsoldier/v2ray-rules-dat@release in Aug 2026; if any URL 404s
// in the future, swap to the meta-rules-dat mirror at the same logical path.

import type { CustomRule, CustomRuleSet } from "./types";

/** Where a preset comes from. Used to group in the picker. */
export type PresetSource = "loyalsoldier" | "meta";

export interface RuleSetPreset {
  /** Unique within source. Becomes the rule-set's `tag` in the generated config. */
  tag: string;
  /** Human label shown in the picker. */
  label: string;
  /** One-line description (e.g. "Chinese IP ranges"). */
  description: string;
  source: PresetSource;
  /** Download URL (.srs binary). */
  url: string;
  /** Human-readable category for grouping. */
  category: "geoip" | "geosite" | "private" | "category";
}

/** All available rule-set presets, ordered as shown in the picker. */
export const RULE_SET_PRESETS: RuleSetPreset[] = [
  // ── Loyalsoldier v2ray-rules-dat (the original we already ship) ──
  {
    tag: "geoip-cn",
    label: "GeoIP — China (Loyalsoldier)",
    description: "All Chinese IP ranges. Used to bypass routing to China.",
    source: "loyalsoldier",
    category: "geoip",
    url: "https://raw.githubusercontent.com/Loyalsoldier/v2ray-rules-dat/release/geoip-cn.srs",
  },
  {
    tag: "geoip-ru",
    label: "GeoIP — Russia (Loyalsoldier)",
    description: "All Russian IP ranges. Used to bypass routing to Russia.",
    source: "loyalsoldier",
    category: "geoip",
    url: "https://raw.githubusercontent.com/Loyalsoldier/v2ray-rules-dat/release/geoip-ru.srs",
  },
  {
    tag: "geosite-ads",
    label: "Geosite — Ads (Loyalsoldier)",
    description: "Ad domains (EasyList etc). Use with action=reject.",
    source: "loyalsoldier",
    category: "geosite",
    url: "https://raw.githubusercontent.com/Loyalsoldier/v2ray-rules-dat/release/geosite-ads.srs",
  },

  // ── Loyalsoldier meta-rules-dat (sing-box 1.10+ compatible, more categories) ──
  {
    tag: "geosite-geolocation-cn",
    label: "Geosite — geolocation-cn (meta)",
    description: "Domains whose IPs are geolocated in China. Wider than geosite-cn.",
    source: "meta",
    category: "geosite",
    url: "https://raw.githubusercontent.com/Loyalsoldier/meta-rules-dat/sing/geo/geosite/geolocation-cn.srs",
  },
  {
    tag: "geosite-cn",
    label: "Geosite — cn (meta)",
    description: "Chinese domains (by name). Most popular Chinese sites.",
    source: "meta",
    category: "geosite",
    url: "https://raw.githubusercontent.com/Loyalsoldier/meta-rules-dat/sing/geo/geosite/cn.srs",
  },
  {
    tag: "geosite-category-ads-all",
    label: "Geosite — ads (meta, all categories)",
    description: "All ad categories from meta-rules-dat (broader than Loyalsoldier basic).",
    source: "meta",
    category: "category",
    url: "https://raw.githubusercontent.com/Loyalsoldier/meta-rules-dat/sing/geo/geosite/category-ads-all.srs",
  },
  {
    tag: "geosite-malware",
    label: "Geosite — malware",
    description: "Known malware / phishing / cryptominer domains.",
    source: "meta",
    category: "category",
    url: "https://raw.githubusercontent.com/Loyalsoldier/meta-rules-dat/sing/geo/geosite/malware.srs",
  },
  {
    tag: "geosite-phishing",
    label: "Geosite — phishing",
    description: "Known phishing domains.",
    source: "meta",
    category: "category",
    url: "https://raw.githubusercontent.com/Loyalsoldier/meta-rules-dat/sing/geo/geosite/phishing.srs",
  },
  {
    tag: "geosite-cryptominers",
    label: "Geosite — cryptominers",
    description: "Cryptojacking / hidden mining script domains.",
    source: "meta",
    category: "category",
    url: "https://raw.githubusercontent.com/Loyalsoldier/meta-rules-dat/sing/geo/geosite/cryptominers.srs",
  },
  {
    tag: "geosite-gfw",
    label: "Geosite — gfw",
    description: "Domains known to be blocked by the GFW in China.",
    source: "meta",
    category: "geosite",
    url: "https://raw.githubusercontent.com/Loyalsoldier/meta-rules-dat/sing/geo/geosite/gfw.srs",
  },
  {
    tag: "geolocation-!cn",
    label: "Geosite — geolocation-!cn (meta)",
    description: "Domains geolocated OUTSIDE China. Use to route 'foreign' traffic.",
    source: "meta",
    category: "geosite",
    url: "https://raw.githubusercontent.com/Loyalsoldier/meta-rules-dat/sing/geo/geosite/geolocation-!cn.srs",
  },
  {
    tag: "private",
    label: "GeoIP — private (RFC1918 + loopback)",
    description: "Private / loopback IP ranges. Use with action=direct for LAN.",
    source: "meta",
    category: "private",
    url: "https://raw.githubusercontent.com/Loyalsoldier/meta-rules-dat/sing/geo/private.srs",
  },
];

/** Convert a preset into an enabled `CustomRuleSet` ready to push into the list. */
export function presetToRuleSet(p: RuleSetPreset): CustomRuleSet {
  return {
    tag: p.tag,
    type: "remote",
    format: "binary",
    url: p.url,
    update_interval: "1d",
    enabled: true,
  };
}

/**
 * "Starter" rule templates — these are the equivalents of the v0.1.0
 * boolean flags. Re-used by the "Add preset rule" picker.
 */
export interface RulePreset {
  /** Stable id, becomes `CustomRule.id`. */
  id: string;
  label: string;
  description: string;
  /** Pre-built CustomRule body. Caller should assign a fresh `id`. */
  build: () => Omit<CustomRule, "id">;
}

export const RULE_PRESETS: RulePreset[] = [
  {
    id: "preset-bypass-lan",
    label: "Bypass LAN",
    description: "Direct route for private/loopback IP ranges (10/8, 172.16/12, 192.168/16, 127/8).",
    build: () => ({
      label: "Bypass LAN",
      enabled: true,
      matchers: {
        ip_cidr: [
          "10.0.0.0/8",
          "172.16.0.0/12",
          "192.168.0.0/16",
          "127.0.0.0/8",
          "169.254.0.0/16",
          "::1/128",
          "fc00::/7",
          "fe80::/10",
        ],
      },
      action: { kind: "route", outbound: "direct" },
    }),
  },
  {
    id: "preset-reject-ipv6",
    label: "Reject IPv6",
    description: "Reject all IPv6 traffic to prevent IPv6 leaks when the proxy doesn't support them.",
    build: () => ({
      label: "Reject IPv6",
      enabled: true,
      matchers: { ip_version: 6 },
      action: { kind: "reject" },
    }),
  },
  {
    id: "preset-block-quic",
    label: "Block QUIC",
    description: "Block UDP/443 (QUIC) to prevent it bypassing the proxy in some networks.",
    build: () => ({
      label: "Block QUIC",
      enabled: true,
      matchers: { port_range: ["443:443"], network: ["udp"] },
      action: { kind: "reject" },
    }),
  },
  {
    id: "preset-bypass-cn",
    label: "Bypass CN (needs geoip-cn rule-set)",
    description: "Direct route for Chinese IPs. Requires the geoip-cn rule-set to be enabled.",
    build: () => ({
      label: "Bypass CN",
      enabled: true,
      matchers: { rule_set: ["geoip-cn"] },
      action: { kind: "route", outbound: "direct" },
    }),
  },
  {
    id: "preset-bypass-ru",
    label: "Bypass RU (needs geoip-ru rule-set)",
    description: "Direct route for Russian IPs. Requires the geoip-ru rule-set to be enabled.",
    build: () => ({
      label: "Bypass RU",
      enabled: true,
      matchers: { rule_set: ["geoip-ru"] },
      action: { kind: "route", outbound: "direct" },
    }),
  },
  {
    id: "preset-block-ads",
    label: "Block Ads (needs geosite-ads rule-set)",
    description: "Reject traffic to ad domains. Requires the geosite-ads rule-set to be enabled.",
    build: () => ({
      label: "Block Ads",
      enabled: true,
      matchers: { rule_set: ["geosite-ads", "geosite-category-ads-all"] },
      action: { kind: "reject" },
    }),
  },
  {
    id: "preset-block-malware",
    label: "Block Malware (needs geosite-malware rule-set)",
    description: "Reject known malware / phishing domains.",
    build: () => ({
      label: "Block Malware",
      enabled: true,
      matchers: { rule_set: ["geosite-malware", "geosite-phishing", "geosite-cryptominers"] },
      action: { kind: "reject" },
    }),
  },
];

/** Generate a unique id for a new rule. */
export function newRuleId(): string {
  return "r-" + Math.random().toString(36).slice(2, 10) + Date.now().toString(36);
}
