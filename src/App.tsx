import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Home,
  Link2,
  Power,
  Route,
  Settings2,
  Terminal,
} from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import cloakwireLogo from "@/assets/cloakwire-logo.png";
import { Button } from "@/components/Button";
import { Badge } from "@/components/Badge";
import { StatusPill } from "@/components/StatusPill";
import { Tabs, type TabDef } from "@/components/Tabs";
import { HomeTab } from "@/components/HomeTab";
import { ServersTab } from "@/components/ServersTab";
import { LogsTab } from "@/components/LogsTab";
import { ConfigTab } from "@/components/ConfigTab";
import { RoutingTab } from "@/components/routing/RoutingTab";
import { TauriCommandError, api } from "@/lib/api";
import { DEFAULT_SETTINGS } from "@/lib/defaults";
import { loadManualProfiles, saveManualProfiles } from "@/lib/manualProfiles";
import {
  buildConnectionProfiles,
  canStartManagedSelection,
  managedSelectionForProfile,
  selectedManualOutbound,
} from "@/lib/connectionProfiles";
import { useSubscriptions } from "@/hooks/useSubscriptions";
import { useGeoIp } from "@/hooks/useGeoIp";
import { useReadyProfileMetadata } from "@/hooks/useReadyProfileMetadata";
import { isSupported } from "@/lib/outbound";
import { basename } from "@/lib/utils";
import { shouldFrontendApplySystemProxy } from "@/lib/systemProxy";
import type {
  BinaryInfo,
  CustomRule,
  CustomRuleSet,
  GeneratorSettings,
  LogLine,
  Outbound,
  ParseFailure,
  RoutingOptions,
  RoutingOptionsV1,
  SingboxVersion,
  Status,
  StatusReport,
} from "@/lib/types";

// Detect whether we're running inside the Tauri shell.
// When opened in a plain browser (e.g. `vite dev` preview) we fall
// back to mock data so the UI is still demoable.
const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const TAB_KEYS = ["home", "servers", "config", "routing", "logs"] as const;
type TabId = (typeof TAB_KEYS)[number];
const DEFAULT_TAB: TabId = "home";

function readStoredTab(): TabId {
  try {
    const v = window.localStorage.getItem("singbox.tab");
    if (v && (TAB_KEYS as readonly string[]).includes(v)) {
      return v as TabId;
    }
  } catch {
    // localStorage may be unavailable in some sandboxes; fall through.
  }
  return DEFAULT_TAB;
}

const SETTINGS_KEY = "singbox-client.settings.v2";
const SETTINGS_KEY_V1 = "singbox-client.settings.v1";

// DEFAULT_SETTINGS is imported from @/lib/defaults — single source
// of truth shared with the Config tab's preview pane.

function freshId(prefix: string) {
  return prefix + "-" + Math.random().toString(36).slice(2, 8) + Date.now().toString(36);
}

/**
 * Convert v0.1.0 boolean flags to a v2 RoutingOptions.
 *
 * Each ON boolean becomes one CustomRule in the new `rules` array. If
 * the rule depends on a rule-set (bypass_cn/geoip-cn, block_ads/
 * geosite-ads, bypass_ru/geoip-ru), the corresponding Loyalsoldier
 * rule-set is added to `rule_sets` so the rule actually fires.
 */
function migrateRoutingV1ToV2(v1: RoutingOptionsV1): RoutingOptions {
  const rules: CustomRule[] = [];
  const ruleSets: CustomRuleSet[] = [];

  // 1. LAN bypass — no rule-set needed.
  if (v1.bypass_lan) {
    rules.push({
      id: freshId("r"),
      label: "Bypass LAN (migrated)",
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
    });
  }

  // 2. IPv6 reject — no rule-set.
  if (v1.reject_ipv6) {
    rules.push({
      id: freshId("r"),
      label: "Reject IPv6 (migrated)",
      enabled: true,
      matchers: { ip_version: 6 },
      action: { kind: "reject" },
    });
  }

  // 3. QUIC reject — no rule-set.
  if (v1.block_quic) {
    rules.push({
      id: freshId("r"),
      label: "Block QUIC (migrated)",
      enabled: true,
      matchers: { port_range: ["443:443"], network: ["udp"] },
      action: { kind: "reject" },
    });
  }

  // 4. CN bypass — needs geoip-cn.
  if (v1.bypass_cn) {
    if (!ruleSets.some((rs) => rs.tag === "geoip-cn")) {
      ruleSets.push({
        tag: "geoip-cn",
        type: "remote",
        format: "binary",
        url: "https://raw.githubusercontent.com/Loyalsoldier/v2ray-rules-dat/release/geoip-cn.srs",
        update_interval: "1d",
        enabled: true,
      });
    }
    rules.push({
      id: freshId("r"),
      label: "Bypass CN (migrated)",
      enabled: true,
      matchers: { rule_set: ["geoip-cn"] },
      action: { kind: "route", outbound: "direct" },
    });
  }

  // 5. RU bypass — needs geoip-ru.
  if (v1.bypass_ru) {
    if (!ruleSets.some((rs) => rs.tag === "geoip-ru")) {
      ruleSets.push({
        tag: "geoip-ru",
        type: "remote",
        format: "binary",
        url: "https://raw.githubusercontent.com/Loyalsoldier/v2ray-rules-dat/release/geoip-ru.srs",
        update_interval: "1d",
        enabled: true,
      });
    }
    rules.push({
      id: freshId("r"),
      label: "Bypass RU (migrated)",
      enabled: true,
      matchers: { rule_set: ["geoip-ru"] },
      action: { kind: "route", outbound: "direct" },
    });
  }

  // 6. Ad blocking — needs geosite-ads.
  if (v1.block_ads) {
    if (!ruleSets.some((rs) => rs.tag === "geosite-ads")) {
      ruleSets.push({
        tag: "geosite-ads",
        type: "remote",
        format: "binary",
        url: "https://raw.githubusercontent.com/Loyalsoldier/v2ray-rules-dat/release/geosite-ads.srs",
        update_interval: "1d",
        enabled: true,
      });
    }
    rules.push({
      id: freshId("r"),
      label: "Block Ads (migrated)",
      enabled: true,
      matchers: { rule_set: ["geosite-ads"] },
      action: { kind: "reject" },
    });
  }

  return {
    rules,
    rule_sets: ruleSets,
    // `migrateRoutingV1ToV2` only knows about boolean flags → rule
    // objects. The "simple" process-picker arrays are always empty
    // after a v1 migration; users populate them on the Routing tab.
    // `final_outbound` is preserved from v1 so the migration is
    // behaviour-preserving (existing v0.1.0 users who had
    // `final_outbound: "proxy"` keep that). The post-merge migration
    // in `loadSettings` will additionally flip a stale `"direct"`
    // back to `"proxy"` for the case where the user never set it
    // explicitly but inherited the broken simple-UX default.
    vpn_processes: [],
    direct_processes: [],
    sniff: true,
    final_outbound: v1.final_outbound || "proxy",
    auto_detect_interface: true,
    default_domain_resolver: "local",
  };
}

function loadSettings(): GeneratorSettings {
  if (typeof window === "undefined") return DEFAULT_SETTINGS;
  try {
    // Try v2 first.
    const rawV2 = window.localStorage.getItem(SETTINGS_KEY);
    if (rawV2) {
      const parsed = JSON.parse(rawV2);
      const merged: GeneratorSettings = {
        ...DEFAULT_SETTINGS,
        ...parsed,
        routing: { ...DEFAULT_SETTINGS.routing, ...(parsed.routing ?? {}) },
        clash_api: { ...DEFAULT_SETTINGS.clash_api, ...(parsed.clash_api ?? {}) },
      };
      // Migration: the simple-UX commit (123e450) flipped the default
      // `final_outbound` from "proxy" to "direct", which inverted the
      // model from "VPN for everything, except apps in Apps direct"
      // to "VPN for nothing, except apps in Apps via VPN". Every
      // existing user opened the app, clicked Connect, and watched all
      // their traffic go direct. If we see `direct` AND the user has
      // no signs of intent (no pickers, no rules, no rule-sets), it
      // was almost certainly the broken default and not a choice —
      // flip it back to "proxy" and persist.
      const r = merged.routing;
      if (
        r.final_outbound === "direct" &&
        r.vpn_processes.length === 0 &&
        r.direct_processes.length === 0 &&
        r.rules.length === 0 &&
        r.rule_sets.length === 0
      ) {
        merged.routing = { ...r, final_outbound: "proxy" };
        try {
          window.localStorage.setItem(SETTINGS_KEY, JSON.stringify(merged));
        } catch { /* ignore */ }
      }
      return merged;
    }
    // Fall back to v1 (silent migration).
    const rawV1 = window.localStorage.getItem(SETTINGS_KEY_V1);
    if (rawV1) {
      try {
        const v1 = JSON.parse(rawV1) as { routing?: RoutingOptionsV1; [k: string]: unknown };
        const v1Routing: RoutingOptionsV1 = v1.routing ?? {
          bypass_lan: true,
          reject_ipv6: true,
          block_quic: false,
          block_ads: false,
          bypass_cn: false,
          bypass_ru: false,
          final_outbound: "proxy",
        };
        const migrated: GeneratorSettings = {
          ...DEFAULT_SETTINGS,
          ...v1,
          routing: migrateRoutingV1ToV2(v1Routing),
        };
        // Persist as v2 immediately and drop v1.
        try {
          window.localStorage.setItem(SETTINGS_KEY, JSON.stringify(migrated));
          window.localStorage.removeItem(SETTINGS_KEY_V1);
        } catch { /* ignore */ }
        return migrated;
      } catch {
        // Corrupt v1 — fall through to defaults.
        return DEFAULT_SETTINGS;
      }
    }
    return DEFAULT_SETTINGS;
  } catch {
    return DEFAULT_SETTINGS;
  }
}

export default function App() {
  const [binary, setBinary] = useState<BinaryInfo | null>(null);
  const [version, setVersion] = useState<SingboxVersion | null>(null);
  const [status, setStatus] = useState<StatusReport>({
    status: "stopped" as Status,
    pid: null,
    uptime_secs: null,
    last_exit_code: null,
    last_error: null,
  });
  const [logs, setLogs] = useState<LogLine[]>([]);
  const [configPath, setConfigPath] = useState<string>("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<TabId>(readStoredTab);
  // Tag of the outbound that the running `proxy` selector is
  // currently routing through. Comes from `GET /proxies/proxy` on
  // the clash API — refreshed alongside status in the poll loop.
  // `null` while sing-box is stopped / unreachable.
  const [activeOutbound, setActiveOutbound] = useState<string | null>(null);

  // Parsed profiles — split into manual + subscription entries
  // so subscription auto-refresh only replaces the slots owned
  // by a particular subscription.
  const [manualProfiles, setManualProfiles] = useState<Outbound[]>(() => loadManualProfiles());
  const [pendingLinks, setPendingLinks] = useState<string>("");
  const [parseErrors, setParseErrors] = useState<ParseFailure[]>([]);
  const [parsing, setParsing] = useState(false);
  // Config-builder settings — lifted here so they survive tab
  // switches and a process restart (persisted to localStorage below).
  const [settings, setSettings] = useState<GeneratorSettings>(loadSettings);
  // Index into `profiles` (manual + subscription) for the server the
  // user has selected on the Home tab. -1 = "auto" (let urltest pick).
  const [selectedIndex, setSelectedIndex] = useState<number>(0);
  const pollTimerRef = useRef<number | null>(null);

  // Persist the settings every time they change.
  useEffect(() => {
    try {
      window.localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
    } catch {
      /* quota exceeded or storage disabled — ignore */
    }
  }, [settings]);

  // Persist manual profiles every time they change. Subscriptions
  // are already persisted in `useSubscriptions`; this is the
  // matching hook for hand-pasted links (`vless://...`). Without
  // this, the user has to re-paste every time they restart the
  // app — which is a real UX gap for a regular user who only
  // uses manual links.
  useEffect(() => {
    saveManualProfiles(manualProfiles);
  }, [manualProfiles]);

  // Subscriptions.
  const subs = useSubscriptions();

  // Persist tab selection across sessions.
  useEffect(() => {
    try {
      window.localStorage.setItem("singbox.tab", activeTab);
    } catch {
      /* ignore quota / disabled storage */
    }
  }, [activeTab]);

  // Manual profiles retain their existing objects; subscription profiles are
  // safe summaries carrying opaque backend references only.
  const profiles = useMemo(
    () => buildConnectionProfiles(manualProfiles, subs.snapshot),
    [manualProfiles, subs.snapshot],
  );
  const readyProfileMetadata = useReadyProfileMetadata(profiles);

  // Online GeoIP fallback for servers the cheap heuristic in
  // `flagForProfile` couldn't resolve (no emoji, no Russian/English
  // name, no TLD). Hits ip-api.com once per uncached IP, then keeps
  // the result in localStorage forever. Lives after `profiles` is
  // declared because it takes the profile list as input.
  const geoip = useGeoIp(manualProfiles);

  const refresh = useCallback(async () => {
    if (!inTauri) {
      // Browser preview — show mock state.
      setBinary({
        path: "C:\\Users\\Алексей\\.minimax-agent\\projects\\singbox-client\\src-tauri\\binaries\\sing-box-x86_64-pc-windows-msvc.exe",
        exists: true,
        size_bytes: 51_732_480,
      });
      setVersion({
        version: "1.14.0-lx.24",
        environment: "go1.26.5 windows/amd64",
        revision: "42e693ce1cbb2f76d611f17fae137c40deaf85fc",
        raw: "sing-box version 1.14.0-lx.24\nEnvironment: go1.26.5 windows/amd64",
      });
      setStatus({
        status: "stopped",
        pid: null,
        uptime_secs: null,
        last_exit_code: null,
        last_error: null,
      });
      setLogs([
        {
          ts: new Date(Date.now() - 60_000).toISOString(),
          stream: "system",
          line: "preview mode — open this in the Tauri shell to control sing-box",
        },
        {
          ts: new Date(Date.now() - 30_000).toISOString(),
          stream: "system",
          line: "backend ready: process manager wired, default config generator in place",
        },
      ]);
      setManualProfiles(demoProfiles());
      setError(null);
      return;
    }
    try {
      // Only poll things that actually change: status + logs.
      // `getBinaryInfo` / `getSingboxVersion` are static (the binary
      // never changes under our feet) so we fetch them exactly once
      // in the mount effect below. Re-fetching them every tick used
      // to spawn `sing-box version` every 1.8 s and flash a CMD
      // window — even after we added CREATE_NO_WINDOW it was still
      // a wasted process; pulling them out of the hot loop removes
      // the flash entirely.
      const [st, log] = await Promise.all([
        api.getStatus(),
        api.getLogs(500),
      ]);
      setStatus(st);
      setLogs(log);
      // While sing-box is up, also pull the currently-active
      // outbound from the clash API. This is what we show as
      // "Active: 🇳🇱 Нидерланды" in the hero — the source of
      // truth for which server the *running* selector is
      // actually using, not just the one the user picked (which
      // may differ if they switched to "Auto" and `urltest`
      // already migrated to a faster server).
      //
      // We serialise behind `getStatus` so the first poll after
      // a restart doesn't try to hit the clash API while the
      // old process is still dying. `listProxies` is allowed to
      // fail silently (the API may be unreachable for a tick
      // during a state transition), in which case we keep the
      // previous `activeOutbound` rather than blanking it out.
      if (st.status === "running") {
        try {
          const p = await api.listProxies();
          const now = p.proxies?.["proxy"]?.now ?? null;
          setActiveOutbound(now);
        } catch {
          // best effort
        }
      } else {
        // Stopped / starting / stopping: clear so the hero
        // doesn't keep showing a stale tag. React skips the
        // re-render if the value is already null.
        setActiveOutbound(null);
      }
      setError(null);
    } catch (e) {
      setError(humanError(e));
    }
  }, []);

  // One-time static fetch: binary path/version. The version literally
  // cannot change while the app is running, so it doesn't belong in
  // the 1.8 s status poll.
  useEffect(() => {
    if (!inTauri) return;
    let cancelled = false;
    (async () => {
      try {
        const [bin, ver] = await Promise.all([
          api.getBinaryInfo(),
          api.getSingboxVersion().catch(() => null),
        ]);
        if (cancelled) return;
        setBinary(bin);
        if (ver) setVersion(ver);
      } catch (e) {
        if (!cancelled) setError(humanError(e));
      }
    })();
    return () => { cancelled = true; };
  }, []);

  /**
   * Re-fetch the sing-box version. Called by the Home tab's
   * `UpdateCard` after a successful sing-box auto-update so the
   * status pill, logs, and version display all reflect the new
   * binary that lives at `<app_data_dir>/singbox-runtime/`.
   *
   * The binary path resolution itself is handled by
   * `ProcessManager::locate_binary` (it now prefers the runtime
   * copy over the bundled one), so we just ask for the version
   * again and trust the result.
   */
  const refetchSingboxVersion = useCallback(async () => {
    if (!inTauri) return;
    try {
      const ver = await api.getSingboxVersion().catch(() => null);
      if (ver) setVersion(ver);
    } catch (e) {
      // Non-fatal — the rest of the UI keeps working with the
      // stale version until the next manual refresh.
      console.error("refetchSingboxVersion failed:", e);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // Polling loop. Faster when starting/stopping, slower when steady.
  useEffect(() => {
    const interval =
      status.status === "starting" || status.status === "stopping" ? 700 : 1800;
    pollTimerRef.current = window.setInterval(() => {
      refresh();
    }, interval);
    return () => {
      if (pollTimerRef.current) window.clearInterval(pollTimerRef.current);
    };
  }, [status.status, refresh]);

  const onStart = useCallback(async () => {
    if (!inTauri) {
      setError("Preview mode — start the Tauri shell to actually run sing-box.");
      return;
    }
    if (!canStartManagedSelection(profiles, selectedIndex)) {
      setError("Selected subscription configuration is not executable yet.");
      return;
    }
    const selection = managedSelectionForProfile(manualProfiles, profiles, selectedIndex);
    if (!selection) {
      setError("Selected subscription configuration is not executable yet.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      // 1. Always generate a fresh config from the imported profiles
      //    + current settings. The config we start with MUST include
      //    the VPN servers, otherwise the proxy accepts connections
      //    but routes them all to `direct`.
      //
      //    `settings.default_outbound` is baked into the proxy
      //    selector's `default`, so the very first request after
      //    sing-box boots goes straight through the picked server.
      //    No more `auto` urltest flash for the first packet.
      const managed = await api.startManaged({
        ...selection,
        settings,
      });
      const path = managed.config_path;
      setConfigPath(path);
      const next = managed.status;
      setStatus(next);

      // 3. Only now that sing-box is alive do we tell Windows to
      //    route traffic through it. If `applySystemProxy` fails,
      //    sing-box is still running and the user can configure
      //    the system proxy manually.
      // Xray owns its dynamically allocated loopback HTTP endpoint in Rust.
      // Replacing it here with the sing-box default port would black-hole all
      // system-proxied traffic. Keep this frontend path only for sing-box.
      if (shouldFrontendApplySystemProxy(next.engine, settings.tunnel_mode)) {
        const port = settings.mixed_port ?? 2080;
        try {
          await api.applySystemProxy("127.0.0.1", port);
        } catch (e) {
          setError(
            `sing-box is running, but the system proxy couldn't be ` +
              `enabled automatically (${humanError(e)}). Set it ` +
              "manually in Settings → Network → Proxy.",
          );
        }
      }

      await refresh();
      setActiveTab("home");
    } catch (e) {
      setError(humanError(e));
    } finally {
      setBusy(false);
    }
  }, [manualProfiles, profiles, selectedIndex, settings, refresh]);

  // Latest onStart, for handlers that need to call it after they've
  // already scheduled a state update (so the new settings object
  // is visible to onStart when it runs).
  const onStartRef = useRef(onStart);
  useEffect(() => {
    onStartRef.current = onStart;
  }, [onStart]);

  const onStop = useCallback(async () => {
    if (!inTauri) return;
    setBusy(true);
    setError(null);
    try {
      // 1. Clear the system proxy FIRST. sing-box is about to die and
      //    Windows DNS / WebView would otherwise sit without a working
      //    resolver for the few seconds it takes the child to exit.
      try {
        await api.clearSystemProxy();
      } catch {
        // best-effort
      }
      // 2. Now ask sing-box to exit. (If we did it the other way
      //    around, traffic would already be failing for ~2s before
      //    Windows noticed it should bypass the proxy again.)
      const next = await api.stop();
      setStatus(next);
      await refresh();
    } catch (e) {
      setError(humanError(e));
    } finally {
      setBusy(false);
    }
  }, [refresh]);

  const onPickConfig = useCallback(async () => {
    if (!inTauri) {
      setError("File picker is only available inside the Tauri shell.");
      return;
    }
    try {
      const picked = await open({
        multiple: false,
        directory: false,
        filters: [
          { name: "sing-box config", extensions: ["json"] },
          { name: "All", extensions: ["*"] },
        ],
      });
      if (typeof picked === "string") {
        setConfigPath(picked);
      }
    } catch (e) {
      setError(humanError(e));
    }
  }, []);

  const onUseDefault = useCallback(() => {
    void (async () => {
      if (!inTauri) {
        setError("Default config writer is only available inside the Tauri shell.");
        return;
      }
      setBusy(true);
      setError(null);
      try {
        const path = await api.writeDefaultConfig();
        setConfigPath(path);
        await api.checkConfig(path);
      } catch (e) {
        setError(humanError(e));
      } finally {
        setBusy(false);
      }
    })();
  }, []);

  // Link / subscription parser (auto-detect).
  const onParseLinks = useCallback(() => {
    void (async () => {
      const text = pendingLinks.trim();
      if (!text) return;
      setParsing(true);
      setError(null);
      setParseErrors([]);
      try {
        if (!inTauri) {
          // Browser preview: tiny client-side mock — treat every
          // non-empty line as a vless:// link.
          const lines = text
            .split(/\r?\n/)
            .map((l) => l.trim())
            .filter((l) => l && !l.startsWith("#"));
          if (lines.length === 0) {
            setError("No entries found in input.");
            return;
          }
          const linkLines = lines.filter((l) => !/^https?:\/\//i.test(l));
          const urlLines = lines.filter((l) => /^https?:\/\//i.test(l));
          if (linkLines.length > 0) {
            const mocks: Outbound[] = linkLines.slice(0, 3).map((l, i) => ({
              protocol: "vless",
              tag: `preview-node-${i + 1}`,
              server: `mock-${i + 1}.example.com`,
              port: 443,
              uuid: "00000000-0000-0000-0000-000000000000",
              flow: "xtls-rprx-vision",
              transport: { kind: "tcp" },
              tls: {
                enabled: true,
                server_name: `mock-${i + 1}.example.com`,
                alpn: ["h2", "http/1.1"],
                reality: { public_key: "PREVIEW", short_id: "abcd" },
                allow_insecure: false,
              },
            }));
            setManualProfiles((prev) => [...mocks, ...prev]);
          }
          for (const u of urlLines) {
            subs.add({ url: u });
          }
          setPendingLinks("");
          return;
        }
        const result = await api.parseInput(text);
        setManualProfiles((prev) => [...result.outbounds, ...prev]);
        setParseErrors(result.failures);
        // Promote detected subscription URLs.
        for (const u of result.subscriptions) {
          try {
            subs.add({ url: u });
          } catch {
            // ignore malformed URL
          }
        }
        if (
          result.outbounds.length > 0 ||
          result.subscriptions.length > 0
        ) {
          setPendingLinks("");
        }
      } catch (e) {
        setError(humanError(e));
      } finally {
        setParsing(false);
      }
    })();
  }, [pendingLinks, inTauri, subs]);

  const onSelectProfile = useCallback(
    async (index: number) => {
      // index === -1 means "Auto (best latency)" — drop the pin,
      // let the `auto` urltest decide.
      const isAuto = index === -1;
      // Resolve the new settings value up front, in two distinct
      // branches, so TypeScript's narrowing is happy in the async
      // closure below and the path is obvious for a human reader.
      const selectedProfile = profiles[index];
      const selectedManual = selectedManualOutbound(profiles, index);
      const pickedTag = isAuto
        ? null
        : selectedManual && isSupported(selectedManual)
          ? selectedManual.tag
          : null;
      setSelectedIndex(index);
      setSettings((prev) => ({
        ...prev,
        // null → urltest, anything else → pinned default in the
        // generated config so the first request after the next
        // Connect goes straight through the picked server.
        default_outbound: pickedTag,
      }));
      if (selectedProfile?.kind === "ready_config") return;
      // If sing-box is already up, take it down cleanly, regenerate
      // the config with the new `default_outbound` baked into the
      // selector's `default`, then bring it back up. This is a
      // 1-2 s gap, but it's the only path that is 100% reliable:
      // `PUT /proxies/proxy` for hot-swapping the selector
      // member is racy on some sing-box versions and the user
      // has reported it sometimes leaves the selector on the old
      // server even though the API returns 204. A full
      // disconnect → regen → reconnect is boring, but boring is
      // exactly what you want from a VPN.
      if (status.status === "running") {
        setBusy(true);
        setError(null);
        try {
          try {
            await api.clearSystemProxy();
          } catch {
            // best effort; we re-apply right after
          }
          const next = await api.stop();
          setStatus(next);
          // Yield so React commits the new `default_outbound`
          // before onStart reads it.
          await Promise.resolve();
          await onStartRef.current();
        } catch (e) {
          setError(humanError(e));
        } finally {
          setBusy(false);
        }
      }
    },
    [profiles, status.status],
  );

  const onRemoveProfile = useCallback((idx: number) => {
    const profile = profiles[idx];
    if (!profile || profile.kind !== "manual") return;
    const manualIndex = profiles
      .slice(0, idx)
      .filter((entry) => entry.kind === "manual").length;
    setManualProfiles((prev) => prev.filter((_, index) => index !== manualIndex));
  }, [profiles]);

  const onClearProfiles = useCallback(() => {
    setManualProfiles([]);
    setParseErrors([]);
  }, []);

  const isRunning = status.status === "running";

  // Tabs are assembled here so they can close over the live state
  // (profiles, status, error, …) without prop-drilling. Order matters.
  const tabs: TabDef[] = useMemo(
    () => [
      {
        id: "home",
        label: "Home",
        icon: Home,
        content: (
          <HomeTab
            status={status}
            statusLabel={status.status}
            busy={busy}
            error={error}
            // The user shouldn't have to know about the config file
            // at all. `onStart` regenerates it from the current
            // profiles + settings on every Connect, so the only
            // real prerequisite is "have at least one server". The
            // tunnel-mode check (admin / Wintun) is handled inside
            // sing-box itself — we just try, and surface any error
            // message in the red banner below the hero.
            canStart={canStartManagedSelection(profiles, selectedIndex)}
            configName={configPath ? basename(configPath) : null}
            profiles={profiles}
            selectedIndex={selectedIndex}
            activeOutbound={activeOutbound}
            readyProfileMetadata={readyProfileMetadata}
            geoipByIp={geoip.byIp}
            currentSingboxVersion={version?.version ?? null}
            onSingboxUpdated={refetchSingboxVersion}
            onSelect={onSelectProfile}
            onConnect={onStart}
            onDisconnect={onStop}
          />
        ),
      },
      {
        id: "servers",
        label: "Servers",
        icon: Link2,
        badge: profiles.length > 0 ? profiles.length : undefined,
        content: (
          <ServersTab
            profiles={profiles}
            parseErrors={parseErrors}
            parseError={error}
            pendingLinks={pendingLinks}
            onPendingLinksChange={setPendingLinks}
            onParse={onParseLinks}
            onRemove={onRemoveProfile}
            onClearAll={onClearProfiles}
            parsing={parsing}
            subs={subs.subs}
            geoipByIp={geoip.byIp}
            subFetching={subs.fetching}
            onAddSub={subs.add}
            onRemoveSub={subs.remove}
            onRefreshSub={subs.refreshOne}
            onRefreshAllSubs={subs.refreshAll}
            onSetSubInterval={subs.setIntervalFor}
            onSelectSubChild={subs.selectChild}
          />
        ),
      },
      {
        id: "config",
        label: "Config",
        icon: Settings2,
        content: (
          <ConfigTab
            configPath={configPath}
            binary={binary}
            version={version}
            statusLabel={status.status}
            profiles={manualProfiles}
            settings={settings}
            onSettingsChange={setSettings}
            onResetSettings={() => setSettings(DEFAULT_SETTINGS)}
            onPickConfig={onPickConfig}
            onUseDefault={onUseDefault}
            onConfigPath={(p) => {
              if (p) setConfigPath(p);
            }}
          />
        ),
      },
      {
        id: "routing",
        label: "Routing",
        icon: Route,
        badge:
          settings.routing.rules.length + settings.routing.rule_sets.length > 0
            ? settings.routing.rules.length + settings.routing.rule_sets.length
            : undefined,
        content: (
          <RoutingTab
            profiles={manualProfiles}
            settings={settings}
            onSettingsChange={setSettings}
          />
        ),
      },
      {
        id: "logs",
        label: "Logs",
        icon: Terminal,
        content: <LogsTab logs={logs} onClear={() => setLogs([])} />,
      },
    ],
    [
      status,
      busy,
      error,
      configPath,
      manualProfiles,
      profiles,
      selectedIndex,
      readyProfileMetadata,
      onStart,
      onStop,
      onSelectProfile,
      pendingLinks,
      parseErrors,
      parsing,
      onParseLinks,
      onRemoveProfile,
      onClearProfiles,
      subs.subs,
      subs.fetching,
      subs.add,
      subs.remove,
      subs.refreshOne,
      subs.refreshAll,
      subs.setIntervalFor,
      subs.selectChild,
      binary,
      version,
      onPickConfig,
      onUseDefault,
      logs,
    ],
  );

  return (
    <div className="relative flex h-screen flex-col overflow-hidden bg-background text-foreground">
      {/* Decorative background grid (matches classquiz) */}
      <div className="pointer-events-none absolute inset-0 bg-grid opacity-30" />

      {/* Header — single slim row, brand + status + controls. */}
      <header className="relative z-10 flex shrink-0 items-center justify-between border-b border-border bg-card/40 px-6 py-2.5 backdrop-blur">
        <div className="flex items-center gap-3">
          <div className="flex h-7 w-7 items-center justify-center overflow-hidden rounded-md bg-primary/15 ring-1 ring-primary/30">
            <img
              src={cloakwireLogo}
              alt="Cloakwire"
              className="h-5 w-5"
            />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h1 className="text-sm font-semibold tracking-tight">
                Cloakwire
              </h1>
              <Badge variant="outline" className="px-1.5 py-0 text-[10px]">
                v1.3.0
              </Badge>
            </div>
            <p className="text-[11px] text-muted-foreground">
              {version
                ? `sing-box ${version.version}`
                : binary?.exists
                  ? basename(binary.path)
                  : "scanning for binary…"}
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <StatusPill
            status={status.status}
            disabled={busy}
            onClick={() => {
              if (busy) return;
              if (status.status === "running") {
                void onStop();
              } else if (
                status.status === "stopped" ||
                status.status === "crashed"
              ) {
                void onStart();
              }
            }}
          />
        </div>
      </header>

      {/* Body — fills the rest of the viewport with the active tab. */}
      <main className="relative z-10 flex min-h-0 flex-1 flex-col">
        <Tabs
          tabs={tabs}
          active={activeTab}
          onChange={(id) => setActiveTab(id as TabId)}
        />
      </main>
    </div>
  );
}

function humanError(e: unknown): string {
  if (e instanceof TauriCommandError) {
    return `${e.kind}: ${e.message}`;
  }
  if (e instanceof Error) return e.message;
  return String(e);
}

function demoProfiles(): Outbound[] {
  return [
    {
      protocol: "vless",
      tag: "🇩🇪 DE-Reality-1",
      server: "de-1.example.com",
      port: 443,
      uuid: "b2a3d6c8-1111-2222-3333-444455556666",
      flow: "xtls-rprx-vision",
      transport: { kind: "grpc", service_name: "" },
      tls: {
        enabled: true,
        server_name: "cdn.example.com",
        alpn: ["h2", "http/1.1"],
        fingerprint: "chrome",
        reality: { public_key: "REAL_PUBLIC_KEY_HERE", short_id: "abcd" },
        allow_insecure: false,
      },
    },
    {
      protocol: "hysteria2",
      tag: "🇳🇱 NL-Hy2-Edge",
      server: "nl-edge.example.org",
      port: 443,
      password: "demo-passphrase",
      tls: {
        enabled: true,
        server_name: "nl-edge.example.org",
        alpn: ["h3"],
        allow_insecure: false,
      },
      obfs: { type: "salamander", password: "obfs-secret" },
    },
    {
      protocol: "shadowsocks",
      tag: "🇸🇬 SG-AES",
      server: "sg.example.net",
      port: 8388,
      method: "chacha20-ietf-poly1305",
      password: "shadowsocks-secret",
    },
    {
      protocol: "trojan",
      tag: "🇺🇸 US-Trojan",
      server: "us.example.com",
      port: 443,
      password: "trojan-pass",
      transport: { kind: "ws", path: "/trojan", headers: [["Host", "us.example.com"]] },
      tls: { enabled: true, server_name: "us.example.com", alpn: [], allow_insecure: false },
    },
  ];
}
