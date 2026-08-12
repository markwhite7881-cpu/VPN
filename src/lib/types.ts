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

export interface RoutingOptions {
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
