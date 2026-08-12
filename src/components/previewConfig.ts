// Browser-preview mirror of the Rust config generator.
//
// We re-implement the same shape (log, dns, inbounds, outbounds, route,
// experimental) in TypeScript so the UI is demoable without Tauri. The
// Rust side is the source of truth — keep the two in sync.

import type {
  GeneratorSettings,
  Outbound,
  Transport,
  TlsCfg,
} from "@/lib/types";

export function previewToSingboxJson(
  outbounds: Outbound[],
  settings: GeneratorSettings,
): Record<string, unknown> {
  const supported = outbounds.filter(
    (o) => o.protocol !== "unsupported",
  );

  // ---- inbounds ----
  const inbounds: Record<string, unknown>[] = [];
  const wantTun =
    settings.tunnel_mode === "tun" || settings.tunnel_mode === "both";
  const wantMixed =
    settings.tunnel_mode === "system_proxy" ||
    settings.tunnel_mode === "both";

  if (wantTun) {
    inbounds.push({
      type: "tun",
      tag: "tun-in",
      // sing-box 1.12+ removed `inet4_address` / `inet6_address`.
      address: ["172.19.0.1/30", "fdfe:dcba:9876::1/126"],
      auto_route: true,
      strict_route: true,
      stack: "system",
      mtu: 9000,
      endpoint_independent_nat: false,
      udp_timeout: "5m",
      interface_name: settings.tun_interface_name ?? "singbox-tun",
    });
  }
  if (wantMixed || (!wantTun && !wantMixed && settings.tunnel_mode === "none")) {
    inbounds.push({
      type: "mixed",
      tag: "mixed-in",
      listen: "127.0.0.1",
      listen_port: settings.mixed_port ?? 2080,
      sniff: true,
      sniff_override_destination: false,
    });
  }

  // ---- outbounds ----
  const profileTags = supported.map((o) =>
    "tag" in o ? o.tag : "(unknown)",
  );

  const outboundsArr: Record<string, unknown>[] = [];
  if (supported.length > 0) {
    outboundsArr.push({
      type: "urltest",
      tag: "auto",
      outbounds: profileTags,
      url: "https://www.gstatic.com/generate_204",
      interval: "3m",
      tolerance: 50,
    });
  }
  outboundsArr.push({
    type: "selector",
    tag: "proxy",
    outbounds:
      supported.length === 0
        ? ["direct"]
        : ["auto", ...profileTags],
    default: "auto",
  });
  outboundsArr.push({ type: "direct", tag: "direct" });
  outboundsArr.push({ type: "block", tag: "block" });
  for (const o of supported) {
    outboundsArr.push(outboundToSingboxJson(o));
  }

  // ---- route ----
  // Routing 2.0: rules and rule-sets come straight from the user's
  // edited list (see `src/components/routing/RoutingTab.tsx`). We just
  // serialize them into the sing-box JSON shape here. The actual
  // authority is Rust (`src-tauri/src/config/mod.rs::generate_config`),
  // which uses the exact same logic. Keep them in sync.
  const rules: Record<string, unknown>[] = [];
  const ruleSets: Record<string, unknown>[] = [];
  const r = settings.routing;
  if (r.sniff) {
    rules.push({ action: "sniff" });
  }
  for (const rule of r.rules) {
    if (!rule.enabled) continue;
    const cleaned: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(rule.matchers)) {
      if (v === undefined || v === null) continue;
      if (Array.isArray(v) && v.length === 0) continue;
      cleaned[k] = v;
    }
    if (Object.keys(cleaned).length === 0) continue;
    if (rule.action.kind === "route") {
      rules.push({ ...cleaned, action: "route", outbound: rule.action.outbound });
    } else if (rule.action.kind === "reject") {
      rules.push({ ...cleaned, action: "reject" });
    } else if (rule.action.kind === "hijack-dns") {
      rules.push({ ...cleaned, action: "hijack-dns" });
    } else if (rule.action.kind === "sniff") {
      rules.push({ ...cleaned, action: "sniff" });
    } else if (rule.action.kind === "resolve") {
      const o: Record<string, unknown> = { ...cleaned, action: "resolve" };
      if (rule.action.server) o.server = rule.action.server;
      if (rule.action.strategy) o.strategy = rule.action.strategy;
      rules.push(o);
    }
  }
  for (const rs of r.rule_sets) {
    if (!rs.enabled) continue;
    const o: Record<string, unknown> = {
      tag: rs.tag,
      type: rs.type,
    };
    if (rs.format) o.format = rs.format;
    if (rs.type === "remote" && rs.url) o.url = rs.url;
    if (rs.type === "local" && rs.path) o.path = rs.path;
    if (rs.update_interval) o.update_interval = rs.update_interval;
    ruleSets.push(o);
  }

  // ---- dns (sing-box 1.12+ typed format) ----
  const localInput = settings.local_dns ?? "77.88.8.8";
  const remoteInput = settings.remote_dns ?? "https://8.8.8.8/dns-query";
  const [localType, localServer] = classifyDns(localInput);
  const [remoteType, remoteServer] = classifyDns(remoteInput);
  const local: Record<string, unknown> = {
    type: localType,
    tag: "local",
    server: localServer,
  };
  const remote: Record<string, unknown> = {
    type: remoteType,
    tag: "remote",
    server: remoteServer,
  };
  if (remoteType === "https") {
    remote.domain_resolver = "local";
  } else {
    remote.detour = "direct";
  }
  const dns = {
    servers: [local, remote],
    final: "remote",
    strategy: "prefer_ipv4",
    independent_cache: true,
  };

  return {
    log: { level: "info", timestamp: true },
    dns,
    inbounds,
    outbounds: outboundsArr,
    route: {
      rules,
      ...(ruleSets.length > 0 ? { rule_set: ruleSets } : {}),
      final: settings.routing.final_outbound,
      auto_detect_interface: settings.routing.auto_detect_interface,
      default_domain_resolver: settings.routing.default_domain_resolver,
    },
    experimental: {
      clash_api: {
        external_controller: settings.clash_api.external_controller,
        default_mode: settings.clash_api.default_controller,
        ...(settings.clash_api.secret
          ? { secret: settings.clash_api.secret }
          : {}),
      },
      cache_file: {
        enabled: true,
        path: "cache.db",
        store_fakeip: false,
      },
    },
  };
}

function outboundToSingboxJson(o: Outbound): Record<string, unknown> {
  if (o.protocol === "unsupported") {
    return { type: "block", tag: "unsupported-placeholder" };
  }
  const out: Record<string, unknown> = {
    type: o.protocol,
    tag: o.tag,
    server: o.server,
    server_port: o.port,
  };
  if ("uuid" in o) out.uuid = o.uuid;
  if ("password" in o) out.password = o.password;
  if ("flow" in o && o.flow) out.flow = o.flow;
  if ("alter_id" in o) out.alter_id = o.alter_id;
  if ("method" in o) out.method = o.method;
  if ("congestion_control" in o) out.congestion_control = o.congestion_control;
  if ("udp_relay_mode" in o) out.udp_relay_mode = o.udp_relay_mode;
  if ("transport" in o) out.transport = outboundTransportToJson(o.transport);
  if ("obfs" in o && o.obfs) out.obfs = o.obfs;
  if ("tls" in o && o.tls?.enabled) {
    const tls: Record<string, unknown> = { enabled: true };
    if (o.tls.server_name) tls.server_name = o.tls.server_name;
    if (o.tls.alpn?.length) tls.alpn = o.tls.alpn;
    if (o.tls.fingerprint) {
      tls.utls = { enabled: true, fingerprint: o.tls.fingerprint };
    }
    if (o.tls.reality) {
      tls.reality = {
        enabled: true,
        public_key: o.tls.reality.public_key,
        short_id: o.tls.reality.short_id,
      };
      if (!tls.utls) {
        tls.utls = { enabled: true, fingerprint: "chrome" };
      }
    }
    if (o.tls.allow_insecure) tls.insecure = true;
    out.tls = tls;
  }
  return out;
}

// Mirror of the Rust `classify_dns` helper.
function classifyDns(s: string): [string, string] {
  if (s.startsWith("https://")) {
    const rest = s.slice("https://".length);
    return ["https", rest.split("/")[0]];
  }
  if (s.startsWith("tls://")) {
    const rest = s.slice("tls://".length);
    return ["tls", rest.split("/")[0]];
  }
  if (s.startsWith("quic://")) {
    const rest = s.slice("quic://".length);
    return ["quic", rest.split("/")[0]];
  }
  return ["udp", s];
}

// Mirror of the Rust `Transport` → sing-box `transport` mapping.
function outboundTransportToJson(
  t: Transport,
): Record<string, unknown> | undefined {
  switch (t.kind) {
    case "tcp":
      return undefined;
    case "ws": {
      const o: Record<string, unknown> = { type: "ws" };
      if (t.path) o.path = t.path;
      if (t.headers?.length) {
        const h: Record<string, string> = {};
        for (const [k, v] of t.headers) h[k] = v;
        o.headers = h;
      }
      return o;
    }
    case "http": {
      const o: Record<string, unknown> = { type: "http" };
      if (t.host?.length) o.host = t.host;
      if (t.path) o.path = t.path;
      return o;
    }
    case "xhttp": {
      const o: Record<string, unknown> = { type: "xhttp" };
      // sing-box-lx (and 1.14+) want a single string `host`,
      // not an array like the legacy `http` transport.
      if (t.host?.length) o.host = t.host[0];
      if (t.path) o.path = t.path;
      if (t.mode) o.mode = t.mode;
      return o;
    }
    case "grpc": {
      const o: Record<string, unknown> = { type: "grpc" };
      if (t.service_name) o.service_name = t.service_name;
      if (t.idle_timeout) o.idle_timeout = t.idle_timeout;
      if (t.ping_timeout) o.ping_timeout = t.ping_timeout;
      return o;
    }
    case "udp":
      return undefined;
  }
}

// Suppress unused-type lints (kept to make this file's types align
// with the Rust impl in future updates).
export type _Mirror = Transport | TlsCfg;
