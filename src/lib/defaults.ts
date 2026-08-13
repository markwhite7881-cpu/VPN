// Default settings shared between the live app (App.tsx) and the
// stand-alone preview pane in ConfigBuilder.tsx.
//
// Single source of truth. App.tsx imports from here for the
// `useState(loadSettings)` initial value, and the preview pane in
// ConfigBuilder.tsx imports the same for its `DEFAULT_SETTINGS` so
// the two never drift apart (which is exactly what happened in
// v0.3.0 — App.tsx was 77.88.8.8 while ConfigBuilder.tsx was 1.1.1.1).
//
// DNS defaults are 77.88.8.8 (Yandex) for users in Russia; Cloudflare
// 1.1.1.1 is a fine global fallback but the user-visible defaults
// have been the Russian ones since 0.3.0.

import type { GeneratorSettings } from "./types";

export const DEFAULT_SETTINGS: GeneratorSettings = {
  tunnel_mode: "system_proxy",
  routing: {
    rules: [],
    rule_sets: [],
    sniff: true,
    final_outbound: "proxy",
    auto_detect_interface: true,
    default_domain_resolver: "local",
  },
  clash_api: {
    external_controller: "127.0.0.1:9090",
    default_controller: "proxy",
    secret: null,
  },
  tun_interface_name: null,
  mixed_port: 2080,
  // 77.88.8.8 (Yandex DNS) — reachable from RU, no on-link-neighbour
  // collision with the TUN's /30 (which the TUN-DNS fix in
  // process.rs::set_tun_dns_from_config additionally guards against
  // at the OS level via `netsh interface ip set dns`).
  local_dns: "77.88.8.8",
  // 8.8.8.8 (Google DNS-over-HTTPS) as the upstream we resolve via
  // the proxy. IP form (not `dns.google`) breaks the circular
  // DNS-for-DNS lookup if the local resolver can't reach the
  // hostname.
  remote_dns: "https://8.8.8.8/dns-query",
  // `null` means "let the `auto` urltest pick the fastest server".
  // The server picker in HomeTab sets this to a real tag on click,
  // which regenerates the config and restarts sing-box so the very
  // first request goes through the picked server (no urltest race).
  default_outbound: null,
};
