// Shapes that mirror the Rust side (see src-tauri/src/parser/mod.rs).
// Tauri serde-deserialises them into these directly.

export type AppError = { kind: string; message: string };

export type Status =
  | "stopped"
  | "starting"
  | "running"
  | "crashed"
  | "stopping";

export interface StatusReport {
  status: Status;
  pid: number | null;
  uptime_secs: number | null;
  last_exit_code: number | null;
  last_error: string | null;
}

export interface LogLine {
  ts: string;
  stream: "stdout" | "stderr" | "system";
  line: string;
}

export interface SingboxVersion {
  version: string;
  environment: string;
  revision: string;
  raw: string;
}

export interface BinaryInfo {
  path: string;
  exists: boolean;
  size_bytes: number;
}

/** One running process. Returned by the `list_processes` Tauri command. */
export interface ProcessInfo {
  pid: number;
  name: string;
}

// --- Parser (
export type Transport =
  | { kind: "tcp" }
  | { kind: "ws"; path?: string; headers: Array<[string, string]> }
  | { kind: "http"; host: string[]; path?: string }
  | {
      kind: "xhttp";
      host: string[];
      path?: string;
      mode?: string;
    }
  | {
      kind: "grpc";
      service_name?: string;
      idle_timeout?: string;
      ping_timeout?: string;
    }
  | { kind: "udp" };

export interface TlsCfg {
  enabled: boolean;
  server_name?: string;
  alpn: string[];
  fingerprint?: string;
  reality?: { public_key: string; short_id: string; spider_x?: string };
  allow_insecure: boolean;
  ech?: { config: string };
}

export interface VlessOut {
  tag: string;
  server: string;
  port: number;
  uuid: string;
  flow?: string;
  transport: Transport;
  tls: TlsCfg;
}

export interface VmessOut {
  tag: string;
  server: string;
  port: number;
  uuid: string;
  alter_id: number;
  cipher: "auto" | "aes128gcm" | "chacha20poly1305" | "none";
  transport: Transport;
  tls: TlsCfg;
}

export interface TrojanOut {
  tag: string;
  server: string;
  port: number;
  password: string;
  transport: Transport;
  tls: TlsCfg;
}

export interface SsOut {
  tag: string;
  server: string;
  port: number;
  method: string;
  password: string;
  plugin?: string;
  plugin_opts?: string;
}

export interface Hy2Out {
  tag: string;
  server: string;
  port: number;
  password: string;
  tls: TlsCfg;
  obfs?: { type: string; password: string };
  up_mbps?: number;
  down_mbps?: number;
}

export interface TuicOut {
  tag: string;
  server: string;
  port: number;
  uuid: string;
  password: string;
  congestion_control: "cubic" | "new_reno" | "bbr";
  udp_relay_mode: "native" | "quic";
  tls: TlsCfg;
}

export type Outbound =
  | ({ protocol: "vless" } & VlessOut)
  | ({ protocol: "vmess" } & VmessOut)
  | ({ protocol: "trojan" } & TrojanOut)
  | ({ protocol: "shadowsocks" } & SsOut)
  | ({ protocol: "hysteria2" } & Hy2Out)
  | ({ protocol: "tuic" } & TuicOut)
  | { protocol: "unsupported"; raw: string; reason: string };

export interface ParseFailure {
  line: string;
  error: AppError;
}

export interface ParseLinksResult {
  outbounds: Outbound[];
  failures: ParseFailure[];
}

/**
 * Result of `parse_input` — handles a mixed blob of share-links
 * AND subscription URLs. HTTP(S) URLs are surfaced separately so
 * the UI can promote them to the subscriptions list.
 */
export interface ParsedInput {
  outbounds: Outbound[];
  /** HTTP(S) URLs to be added as subscriptions. */
  subscriptions: string[];
  failures: ParseFailure[];
}

// --- 
export type TunnelMode = "tun" | "system_proxy" | "both" | "none";

/**
 * Routing 2.0 — flat per-rule + rule-set list.
 *
 * Replaces the old `RoutingOptions` of 7 boolean flags. Existing
 * v0.1.0 settings are migrated silently in `App.tsx#loadSettings`.
 *
 * Shape mirrors sing-box 1.14+ config (`route.rules[]`, `route.rule_set[]`)
 * with a couple of UI-only fields on `CustomRule`:
 *   - `id` — stable React key
 *   - `label` — human-friendly name shown in the list
 *   - `enabled` — UI-only toggle; excluded rules are filtered out before
 *     generating config, not serialized as `invert: true`
 */

/** Action attached to a rule. Mirrors sing-box rule_action. */
export type RuleAction =
  | { kind: "route"; outbound: string }
  | { kind: "reject" }
  | { kind: "hijack-dns" }
  | { kind: "sniff"; sniffer?: string[]; timeout?: string }
  | { kind: "resolve"; server?: string; strategy?: string };

/** All matchers supported in this build (sing-box 1.14+). */
export interface RuleMatchers {
  // network / inbound
  inbound?: string[];
  ip_version?: 4 | 6;
  network?: ("tcp" | "udp" | "icmp")[];
  auth_user?: string[];
  // domain
  domain?: string[];
  domain_suffix?: string[];
  domain_keyword?: string[];
  domain_regex?: string[];
  // ip
  ip_cidr?: string[];
  source_ip_cidr?: string[];
  ip_is_private?: boolean;
  source_ip_is_private?: boolean;
  // port
  port?: number[];
  port_range?: string[];
  source_port?: number[];
  source_port_range?: string[];
  // process (Linux/Win/Mac)
  process_name?: string[];
  process_path?: string[];
  process_path_regex?: string[];
  // sniff
  protocol?: string[];
  client?: string[];
  // reference
  rule_set?: string[];
  rule_set_ip_cidr_match_source?: boolean;
}

/** One user-visible rule in the routing list. */
export interface CustomRule {
  /** Stable id, used as React key. */
  id: string;
  /** Human label, e.g. "Telegram" or "Work split-tunnel". */
  label?: string;
  /** UI-only — disabled rules are skipped at config-generation time. */
  enabled: boolean;
  matchers: RuleMatchers;
  /**
   * sing-box `invert` flag — match everything *except* these
   * conditions. Lives on the rule (not the matchers) per sing-box 1.14+.
   */
  invert?: boolean;
  action: RuleAction;
}

/** A reference to a sing-box rule-set. */
export interface CustomRuleSet {
  /** Unique tag referenced from `RuleMatchers.rule_set`. */
  tag: string;
  type: "remote" | "local" | "inline";
  format?: "source" | "binary";
  url?: string;
  path?: string;
  /** Inline rules (only when type === "inline"). */
  rules?: RuleMatchers[];
  /** Update interval, e.g. "1d". Defaults to "1d" for remote. */
  update_interval?: string;
  /** UI-only. Disabled rule-sets are omitted at config-generation time. */
  enabled: boolean;
}

/**
 * Routing 2.0 — full routing config.
 *
 * Note: this is intentionally a *replacement* for the v0.1.0 boolean
 * shape. See `migrateRoutingV1ToV2` in `App.tsx` for the conversion.
 */
export interface RoutingOptions {
  /** Ordered rule list (first match wins). */
  rules: CustomRule[];
  /** External rule-sets (Loyalsoldier / meta-rules-dat / custom URL). */
  rule_sets: CustomRuleSet[];
  /**
   * Process names (e.g. `"telegram.exe"`, `"chrome"`) that should
   * route through the VPN. Synthesised as a `process_name` rule
   * that matches FIRST in the generated `route.rules` (before
   * anything in `rules[]`). Empty by default.
   *
   * The default outbound for matched traffic is `auto` (urltest
   * picks the fastest server), so the user doesn't need to pin a
   * specific server.
   */
  vpn_processes: string[];
  /**
   * Process names that should bypass the VPN (go direct). Same
   * shape as `vpn_processes` but synthesised as a rule that
   * routes to `direct`. Matches FIRST so a process can't be in
   * both lists (the more specific "direct" wins). Empty by default.
   */
  direct_processes: string[];
  /** Always-on sniff action pushed at the top of `route.rules`. */
  sniff: boolean;
  /** `route.final` outbound tag. */
  final_outbound: string;
  /** `route.auto_detect_interface` — prevents TUN routing loop on Win/Mac/Linux. */
  auto_detect_interface: boolean;
  /** `route.default_domain_resolver` tag (usually "local"). */
  default_domain_resolver: string;
  /**
   * Android only: which apps the VpnService captures at all.
   * Values mirror the Rust side (`config/mod.rs`): "all" (default),
   * "include" (only `tun_app_list` uses the VPN), "exclude"
   * (everything except `tun_app_list`). Ignored on desktop.
   */
  tun_app_mode?: "all" | "include" | "exclude";
  /** Android only: package names for `tun_app_mode`. */
  tun_app_list?: string[];
}

/** v0.1.0 routing shape — only used by the silent migration. */
export interface RoutingOptionsV1 {
  bypass_lan: boolean;
  reject_ipv6: boolean;
  block_ads: boolean;
  bypass_cn: boolean;
  bypass_ru: boolean;
  block_quic: boolean;
  final_outbound: string;
}

export interface ClashApiOptions {
  external_controller: string;
  default_controller: string;
  secret: string | null;
}

export interface GeneratorSettings {
  tunnel_mode: TunnelMode;
  routing: RoutingOptions;
  clash_api: ClashApiOptions;
  tun_interface_name: string | null;
  mixed_port: number | null;
  local_dns: string | null;
  remote_dns: string | null;
  /**
   * Tag of the outbound that the `proxy` selector should boot
   * pinned to. `null` or `"auto"` → `auto` urltest decides.
   * Any other value → the matching server is used as the
   * default so the very first request after sing-box starts
   * goes through it (no urltest "flash").
   */
  default_outbound: string | null;
}

// --- 
export interface DelayRecord {
  time: string;
  delay: number;
}

export interface ProxyInfo {
  type: string; // "Selector" | "URLTest" | "VLESS" | "Direct" | "Block" | …
  all: string[];
  now: string | null;
  history: DelayRecord[];
}

export interface ProxiesResponse {
  proxies: Record<string, ProxyInfo>;
}

// --- 
export interface TrafficSample {
  up_bps: number;
  down_bps: number;
  up_total: number;
  down_total: number;
  ts_ms: number;
}

// --- 
export interface Subscription {
  /** Stable id, used as the React key. */
  id: string;
  /** Human label, e.g. "Work proxy". */
  name: string;
  /** Full URL the provider gave us. */
  url: string;
  /** Auto-refresh interval in minutes. 0 disables. */
  intervalMinutes: number;
  /** ISO timestamp of the last successful fetch. */
  lastFetchedAt: string | null;
  /** Number of profiles last fetched. */
  lastCount: number;
  /** Last error message (if any). */
  lastError: string | null;
  /** Last error kind (parse / network / etc.). */
  lastErrorKind: string | null;
}
