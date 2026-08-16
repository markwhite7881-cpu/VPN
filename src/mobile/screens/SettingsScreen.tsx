import { useEffect, useState } from "react";
import { RefreshCw, Terminal } from "lucide-react";
import { vpnCoreVersion } from "@/lib/vpn";
import { cn } from "@/lib/utils";
import type { CustomRule, GeneratorSettings, RoutingOptions } from "@/lib/types";
import { newRuleId } from "@/lib/presets";
import { SectionCard, SectionHeader, SettingRow } from "../components/SectionCard";
import { Switch } from "../components/Switch";

const inTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** DNS provider presets — write both local and remote upstreams. */
const DNS_PRESETS: { id: string; label: string; local: string; remote: string }[] = [
  {
    id: "yandex",
    label: "Yandex (77.88.8.8)",
    local: "77.88.8.8",
    remote: "https://8.8.8.8/dns-query",
  },
  {
    id: "cloudflare",
    label: "Cloudflare (1.1.1.1)",
    local: "1.1.1.1",
    remote: "https://1.1.1.1/dns-query",
  },
  {
    id: "google",
    label: "Google (8.8.8.8)",
    local: "8.8.8.8",
    remote: "https://dns.google/dns-query",
  },
];

const IPV6_RULE_LABEL = "Reject IPv6";

function isIpv6RejectRule(rule: CustomRule): boolean {
  return (
    rule.action.kind === "reject" &&
    rule.matchers.ip_version === 6 &&
    Object.keys(rule.matchers).length === 1
  );
}

export function SettingsScreen({
  settings,
  onSettingsChange,
  autoConnect,
  onAutoConnectChange,
  onRefreshAllSubs,
  subsFetching,
  onOpenLogs,
}: {
  settings: GeneratorSettings;
  onSettingsChange: (next: GeneratorSettings) => void;
  autoConnect: boolean;
  onAutoConnectChange: (v: boolean) => void;
  onRefreshAllSubs: () => void;
  subsFetching: boolean;
  onOpenLogs: () => void;
}) {
  const update = (patch: Partial<GeneratorSettings>) =>
    onSettingsChange({ ...settings, ...patch });
  const updateRouting = (patch: Partial<RoutingOptions>) =>
    onSettingsChange({
      ...settings,
      routing: { ...settings.routing, ...patch },
    });

  const [coreVersion, setCoreVersion] = useState<string | null>(null);
  useEffect(() => {
    if (!inTauri) return;
    let cancelled = false;
    vpnCoreVersion()
      .then((v) => {
        if (!cancelled) setCoreVersion(v);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  // IPv6 switch: presence of an enabled reject-ip_version:6 rule.
  const ipv6Blocked = settings.routing.rules.some(
    (r) => isIpv6RejectRule(r) && r.enabled,
  );
  const setIpv6Blocked = (blocked: boolean) => {
    const rules = settings.routing.rules;
    if (blocked) {
      if (ipv6Blocked) return;
      updateRouting({
        rules: [
          ...rules,
          {
            id: newRuleId(),
            label: IPV6_RULE_LABEL,
            enabled: true,
            matchers: { ip_version: 6 },
            action: { kind: "reject" },
          },
        ],
      });
    } else {
      updateRouting({ rules: rules.filter((r) => !isIpv6RejectRule(r)) });
    }
  };

  // Remote rule-sets switch: enables/disables every remote rule-set.
  const ruleSets = settings.routing.rule_sets;
  const remoteSets = ruleSets.filter((rs) => rs.type === "remote");
  const remoteEnabled =
    remoteSets.length > 0 && remoteSets.every((rs) => rs.enabled);
  const setRemoteEnabled = (v: boolean) =>
    updateRouting({
      rule_sets: ruleSets.map((rs) =>
        rs.type === "remote" ? { ...rs, enabled: v } : rs,
      ),
    });

  const dnsPreset =
    DNS_PRESETS.find(
      (p) => p.local === settings.local_dns && p.remote === settings.remote_dns,
    )?.id ?? "custom";

  const inputCls =
    "rounded-md border border-input bg-background px-2.5 py-1.5 text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-ring";

  return (
    <div className="flex flex-col gap-4 p-4">
      <SectionCard>
        <SectionHeader title="General" />
        <div className="divide-y divide-border">
          <SettingRow
            label="Auto-connect"
            hint="Connect on app start."
            control={
              <Switch
                checked={autoConnect}
                onChange={onAutoConnectChange}
                label="Auto-connect"
              />
            }
          />
          <SettingRow
            label="Block IPv6"
            hint="Reject IPv6 traffic to prevent leaks."
            control={
              <Switch
                checked={ipv6Blocked}
                onChange={setIpv6Blocked}
                label="Block IPv6"
              />
            }
          />
          <SettingRow
            label="Remote rule-sets"
            hint={
              remoteSets.length === 0
                ? "No rule-sets configured."
                : `${remoteSets.length} rule-set${remoteSets.length === 1 ? "" : "s"} — download updates over the network.`
            }
            control={
              <Switch
                checked={remoteEnabled}
                onChange={setRemoteEnabled}
                disabled={remoteSets.length === 0}
                label="Remote rule-sets"
              />
            }
          />
        </div>
      </SectionCard>

      <SectionCard>
        <SectionHeader title="Network" />
        <div className="divide-y divide-border">
          <SettingRow
            label="DNS provider"
            control={
              <select
                value={dnsPreset}
                onChange={(e) => {
                  const p = DNS_PRESETS.find((x) => x.id === e.target.value);
                  if (p) update({ local_dns: p.local, remote_dns: p.remote });
                }}
                className={cn(inputCls, "max-w-[180px]")}
              >
                {DNS_PRESETS.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.label}
                  </option>
                ))}
                {dnsPreset === "custom" && (
                  <option value="custom">Custom</option>
                )}
              </select>
            }
          />
          <SettingRow
            label="Mixed port"
            hint="Local SOCKS/HTTP listener."
            control={
              <input
                type="number"
                inputMode="numeric"
                min={1}
                max={65535}
                value={settings.mixed_port ?? 2080}
                onChange={(e) => {
                  const n = Number(e.target.value);
                  if (Number.isInteger(n) && n >= 1 && n <= 65535) {
                    update({ mixed_port: n });
                  }
                }}
                className={cn(inputCls, "w-24 text-right font-mono")}
              />
            }
          />
        </div>
      </SectionCard>

      <SectionCard>
        <SectionHeader title="About" />
        <div className="divide-y divide-border">
          <SettingRow
            label="Core"
            hint="sing-box (libbox)"
            control={
              <span className="font-mono text-xs text-muted-foreground">
                {coreVersion ?? (inTauri ? "…" : "preview")}
              </span>
            }
          />
          <div className="px-4 py-3">
            <button
              type="button"
              onClick={onOpenLogs}
              className="flex h-9 w-full items-center justify-center gap-2 rounded-md border border-border text-sm text-foreground hover:bg-accent"
            >
              <Terminal className="h-3.5 w-3.5" />
              View logs
            </button>
          </div>
          <div className="px-4 py-3">
            <button
              type="button"
              onClick={onRefreshAllSubs}
              disabled={subsFetching}
              className="flex h-9 w-full items-center justify-center gap-2 rounded-md border border-border text-sm text-foreground hover:bg-accent disabled:opacity-50"
            >
              <RefreshCw
                className={cn("h-3.5 w-3.5", subsFetching && "animate-spin")}
              />
              Check subscription updates
            </button>
          </div>
        </div>
      </SectionCard>
    </div>
  );
}
