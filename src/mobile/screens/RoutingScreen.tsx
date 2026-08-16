import { useEffect, useMemo, useState } from "react";
import { ChevronDown, Plus, Search, X } from "lucide-react";
import { vpnListApps, type AppEntry } from "@/lib/vpn";
import { newRuleId } from "@/lib/presets";
import { cn } from "@/lib/utils";
import { orderAppsForPicker, summarizeSelectedApps } from "../lib/mobileUi";
import type {
  CustomRule,
  GeneratorSettings,
  RoutingOptions,
} from "@/lib/types";
import { SectionCard, SectionHeader } from "../components/SectionCard";
import { Switch } from "../components/Switch";

const inTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type TunAppMode = "all" | "include" | "exclude";

const APP_MODES: { id: TunAppMode; label: string }[] = [
  { id: "all", label: "All apps" },
  { id: "include", label: "Only selected" },
  { id: "exclude", label: "All except selected" },
];

/**
 * Global mode selector. The Settings model has no dedicated
 * `routing_mode` field — the closest existing knob is
 * `routing.final_outbound`: "proxy" = everything via the selector,
 * "auto" = urltest picks, "direct" = everything direct.
 */
const GLOBAL_MODES = [
  { id: "proxy", label: "Global", hint: "Everything through the selected server" },
  { id: "auto", label: "Auto", hint: "Fastest server (urltest)" },
  { id: "direct", label: "Direct", hint: "Everything bypasses the VPN" },
] as const;

export function RoutingScreen({
  settings,
  onSettingsChange,
}: {
  settings: GeneratorSettings;
  onSettingsChange: (next: GeneratorSettings) => void;
}) {
  const r = settings.routing;
  const updateRouting = (patch: Partial<RoutingOptions>) =>
    onSettingsChange({ ...settings, routing: { ...r, ...patch } });

  const mode: TunAppMode = r.tun_app_mode ?? "all";
  const appList = r.tun_app_list ?? [];

  return (
    <div className="flex flex-col gap-4 p-4">
      {/* Mode */}
      <SectionCard>
        <SectionHeader
          title="Mode"
          description="Where traffic goes when no rule matches."
        />
        <div className="grid grid-cols-3 gap-1.5 p-3">
          {GLOBAL_MODES.map((m) => {
            const active = r.final_outbound === m.id;
            return (
              <button
                key={m.id}
                type="button"
                onClick={() => updateRouting({ final_outbound: m.id })}
                aria-pressed={active}
                title={m.hint}
                className={cn(
                  "rounded-md border px-2 py-2 text-xs font-medium transition-colors",
                  active
                    ? "border-foreground/40 bg-foreground/10 text-foreground"
                    : "border-border text-muted-foreground hover:bg-accent hover:text-foreground",
                )}
              >
                {m.label}
              </button>
            );
          })}
        </div>
      </SectionCard>

      {/* Per-app routing */}
      <PerAppSection
        mode={mode}
        appList={appList}
        hasRouteRules={r.rules.length > 0}
        onModeChange={(tun_app_mode) => updateRouting({ tun_app_mode })}
        onListChange={(tun_app_list) => updateRouting({ tun_app_list })}
      />

      {/* Rule groups */}
      <RuleGroup
        title="Proxy rules"
        hint="Domains / IPs routed through the VPN."
        group="proxy"
        rules={r.rules}
        onChange={(rules) => updateRouting({ rules })}
      />
      <RuleGroup
        title="Direct rules"
        hint="Domains / IPs that bypass the VPN."
        group="direct"
        rules={r.rules}
        onChange={(rules) => updateRouting({ rules })}
      />
      <RuleGroup
        title="Block rules"
        hint="Domains / IPs rejected entirely."
        group="block"
        rules={r.rules}
        onChange={(rules) => updateRouting({ rules })}
      />
    </div>
  );
}

// ---- Per-app VPN --------------------------------------------------

function PerAppSection({
  mode,
  appList,
  hasRouteRules,
  onModeChange,
  onListChange,
}: {
  mode: TunAppMode;
  appList: string[];
  hasRouteRules: boolean;
  onModeChange: (m: TunAppMode) => void;
  onListChange: (l: string[]) => void;
}) {
  const [apps, setApps] = useState<AppEntry[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [expanded, setExpanded] = useState(false);

  useEffect(() => {
    if (!expanded || apps != null || !inTauri) return;
    let cancelled = false;
    setLoading(true);
    vpnListApps()
      .then((list) => {
        if (cancelled) return;
        setApps(list);
      })
      .catch((e) => {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [expanded, apps]);

  const selected = useMemo(() => new Set(appList), [appList]);
  const filtered = useMemo(() => {
    if (!apps) return [];
    const q = query.trim().toLowerCase();
    if (!q) return apps;
    return apps.filter(
      (a) =>
        a.label.toLowerCase().includes(q) ||
        a.packageName.toLowerCase().includes(q),
    );
  }, [apps, query]);

  const visibleApps = useMemo(
    () => orderAppsForPicker(filtered, selected, mode),
    [filtered, mode, selected],
  );
  const pickerLabel = summarizeSelectedApps(apps, appList);

  const toggle = (pkg: string) => {
    const next = new Set(selected);
    if (next.has(pkg)) next.delete(pkg);
    else next.add(pkg);
    onListChange(Array.from(next));
  };

  return (
    <SectionCard>
      <SectionHeader title="Per-app routing" />
      <div className="space-y-3 p-3">
        <div className="grid grid-cols-3 gap-1.5">
          {APP_MODES.map((m) => (
            <button
              key={m.id}
              type="button"
              onClick={() => onModeChange(m.id)}
              aria-pressed={mode === m.id}
              className={cn(
                "rounded-md border px-2 py-2 text-[11px] font-medium transition-colors",
                mode === m.id
                  ? "border-foreground/40 bg-foreground/10 text-foreground"
                  : "border-border text-muted-foreground hover:bg-accent hover:text-foreground",
              )}
            >
              {m.label}
            </button>
          ))}
        </div>

        {!hasRouteRules && appList.length === 0 && (
          <div className="rounded-md border border-border bg-accent/40 px-3 py-2 text-xs text-muted-foreground">
            Add a route rule or select apps to customize routing.
          </div>
        )}

        {mode !== "all" && (
          <>
            <button
              type="button"
              onClick={() => setExpanded((v) => !v)}
              className="flex w-full items-center justify-between rounded-md border border-border px-3 py-2 text-xs text-foreground hover:bg-accent"
            >
              <span>{pickerLabel}</span>
              <ChevronDown
                className={cn(
                  "h-3.5 w-3.5 text-muted-foreground transition-transform",
                  expanded && "rotate-180",
                )}
              />
            </button>

            {expanded && (
              <div className="space-y-2">
                <div className="flex items-center gap-2 rounded-md border border-input bg-background px-2.5">
                  <Search className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                  <input
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    placeholder="Search apps"
                    className="h-8 w-full bg-transparent text-xs text-foreground placeholder:text-muted-foreground/60 focus:outline-none"
                  />
                </div>
                {error && <p className="text-xs text-destructive">{error}</p>}
                {loading && (
                  <p className="py-4 text-center text-xs text-muted-foreground">
                    Loading apps…
                  </p>
                )}
                {!inTauri && (
                  <p className="py-4 text-center text-xs text-muted-foreground">
                    App list is only available on Android.
                  </p>
                )}
                {apps != null && visibleApps.length === 0 && !loading && (
                  <p className="py-4 text-center text-xs text-muted-foreground">
                    No apps match.
                  </p>
                )}
                {apps != null && (
                  <ul className="max-h-72 divide-y divide-border overflow-y-auto rounded-md border border-border">
                    {visibleApps.map((a) => (
                      <li key={a.packageName}>
                        <button
                          type="button"
                          onClick={() => toggle(a.packageName)}
                          className="flex w-full items-center gap-2.5 px-3 py-2 text-left hover:bg-accent/60"
                        >
                          <span
                            className={cn(
                              "flex h-4 w-4 shrink-0 items-center justify-center rounded border",
                              selected.has(a.packageName)
                                ? "border-foreground/40 bg-foreground/10"
                                : "border-muted-foreground/40",
                            )}
                            aria-hidden
                          >
                            {selected.has(a.packageName) && (
                              <span className="h-2 w-2 rounded-sm bg-foreground" />
                            )}
                          </span>
                          <span className="min-w-0 flex-1">
                            <span className="block truncate text-xs font-medium">
                              {a.label}
                            </span>
                            <span className="block truncate font-mono text-[10px] text-muted-foreground">
                              {a.packageName}
                            </span>
                          </span>
                        </button>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            )}
          </>
        )}
      </div>
    </SectionCard>
  );
}

// ---- Rule groups ----------------------------------------------------

type RuleGroupId = "proxy" | "direct" | "block";

function ruleGroup(rule: CustomRule): RuleGroupId | null {
  if (rule.action.kind === "reject") return "block";
  if (rule.action.kind === "route") {
    return rule.action.outbound === "direct" ? "direct" : "proxy";
  }
  return null; // sniff / resolve / hijack-dns — not shown in groups
}

function summarizeMatchers(rule: CustomRule): string {
  const m = rule.matchers;
  const parts: string[] = [];
  if (m.domain?.length) parts.push(...m.domain);
  if (m.domain_suffix?.length) parts.push(...m.domain_suffix.map((d) => `*.${d}`));
  if (m.domain_keyword?.length) parts.push(...m.domain_keyword.map((d) => `*${d}*`));
  if (m.ip_cidr?.length) parts.push(...m.ip_cidr);
  if (m.rule_set?.length) parts.push(...m.rule_set.map((t) => `[${t}]`));
  if (m.port?.length) parts.push(...m.port.map((p) => `:${p}`));
  if (m.process_name?.length) parts.push(...m.process_name);
  return parts.join(", ") || "all traffic";
}

/** Guess the matcher for a typed value: CIDR/IP → ip_cidr, else domain_suffix. */
function buildRule(group: RuleGroupId, value: string): CustomRule {
  const isIp = /^[\d:.\/a-fA-F]+$/.test(value) && /[\d]/.test(value) && (value.includes("/") || value.includes(".") || value.includes(":"));
  return {
    id: newRuleId(),
    label: value,
    enabled: true,
    matchers: isIp ? { ip_cidr: [value] } : { domain_suffix: [value] },
    action:
      group === "block"
        ? { kind: "reject" }
        : { kind: "route", outbound: group === "direct" ? "direct" : "proxy" },
  };
}

function RuleGroup({
  title,
  hint,
  group,
  rules,
  onChange,
}: {
  title: string;
  hint: string;
  group: RuleGroupId;
  rules: CustomRule[];
  onChange: (rules: CustomRule[]) => void;
}) {
  const [value, setValue] = useState("");
  const mine = rules.filter((r) => ruleGroup(r) === group);

  const add = () => {
    const v = value.trim();
    if (!v) return;
    onChange([...rules, buildRule(group, v)]);
    setValue("");
  };

  return (
    <details className="group rounded-lg border border-border bg-card open:bg-card">
      <summary className="flex cursor-pointer select-none list-none items-center gap-2 px-4 py-3 text-sm font-semibold tracking-tight [&::-webkit-details-marker]:hidden">
        <span className="advanced-arrow text-xs text-muted-foreground" />
        {title}
        <span className="text-[10px] font-normal text-muted-foreground">
          {mine.length}
        </span>
      </summary>
      <div className="space-y-2 border-t border-border px-3 pb-3 pt-2.5">
        <p className="text-[11px] text-muted-foreground">{hint}</p>
        {mine.length === 0 ? (
          <p className="py-1 text-[11px] italic text-muted-foreground/70">
            No entries.
          </p>
        ) : (
          <ul className="divide-y divide-border rounded-md border border-border">
            {mine.map((rule) => (
              <li key={rule.id} className="flex items-center gap-2 px-3 py-2">
                <div className="min-w-0 flex-1">
                  <p className="truncate text-xs font-medium">
                    {rule.label || "rule"}
                  </p>
                  <p className="truncate font-mono text-[10px] text-muted-foreground">
                    {summarizeMatchers(rule)}
                  </p>
                </div>
                <Switch
                  checked={rule.enabled}
                  label={`Enable ${rule.label || "rule"}`}
                  onChange={(v) =>
                    onChange(
                      rules.map((r) =>
                        r.id === rule.id ? { ...r, enabled: v } : r,
                      ),
                    )
                  }
                />
                <button
                  type="button"
                  onClick={() => onChange(rules.filter((r) => r.id !== rule.id))}
                  aria-label={`Remove ${rule.label || "rule"}`}
                  className="flex h-6 w-6 items-center justify-center rounded text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              </li>
            ))}
          </ul>
        )}
        <div className="flex items-center gap-2">
          <input
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") add();
            }}
            placeholder={group === "block" ? "ads.example.com" : "example.com or 1.2.3.0/24"}
            className="h-8 w-full rounded-md border border-input bg-background px-2.5 font-mono text-xs text-foreground placeholder:text-muted-foreground/50 focus:outline-none focus:ring-1 focus:ring-ring"
          />
          <button
            type="button"
            onClick={add}
            disabled={!value.trim()}
            aria-label="Add entry"
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          >
            <Plus className="h-4 w-4" />
          </button>
        </div>
      </div>
    </details>
  );
}
