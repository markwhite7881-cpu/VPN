// RoutingTab — main container for the "Routing" page.
//
// The simple UX (top of the page, what 99% of users see):
//
//   ┌─────────────────────────────────────────────────────────────┐
//   │  Apps via VPN                                               │
//   │  [Telegram ✕] [Chrome ✕]      [Pick from running… ▾]        │
//   │  → these processes route through VPN; everything else direct │
//   └─────────────────────────────────────────────────────────────┘
//   ┌─────────────────────────────────────────────────────────────┐
//   │  Apps direct                                                │
//   │  [Bank ✕]                 [Pick from running… ▾]            │
//   │  → force these processes to bypass VPN (e.g. bank apps)     │
//   └─────────────────────────────────────────────────────────────┘
//
//   <details>
//     <summary>Advanced</summary>
//     …full rule editor (matchers, drag-and-drop, presets,
//       rule-sets, sniff/final_outbound/JSON preview).
//   </details>
//
// The "Advanced" section is collapsed by default — non-tech-savvy
// users never see it. The simple UX is the answer to "just let me
// pick which programs use VPN and which don't, nothing else".

import { useMemo, useState } from "react";
import { Copy, RotateCcw, Shield, ShieldOff, X } from "lucide-react";
import { Button } from "../Button";
import { RuleList } from "./RuleList";
import { PresetPicker } from "./PresetPicker";
import { RuleSetsPanel } from "./RuleSetsPanel";
import { ProcessPicker } from "./ProcessPicker";
import { newRuleId } from "@/lib/presets";
import type { CustomRule, CustomRuleSet, GeneratorSettings, Outbound, RoutingOptions } from "@/lib/types";
import { cn } from "@/lib/utils";

interface Props {
  profiles: Outbound[];
  settings: GeneratorSettings;
  onSettingsChange: (next: GeneratorSettings) => void;
}

export function RoutingTab({ profiles, settings, onSettingsChange }: Props) {
  const r = settings.routing;
  const updateRouting = (patch: Partial<RoutingOptions>) =>
    onSettingsChange({ ...settings, routing: { ...r, ...patch } });

  const [jsonOpen, setJsonOpen] = useState(false);
  const [jsonCopied, setJsonCopied] = useState(false);
  const [advancedOpen, setAdvancedOpen] = useState(false);

  // Build a sing-box-style JSON view of the routing config.
  // This is purely informational — the *real* generation is in Rust.
  const jsonPreview = useMemo(() => buildJsonPreview(r), [r]);

  const onAddRule = () => {
    const rule: CustomRule = {
      id: newRuleId(),
      label: "New rule",
      enabled: true,
      matchers: {},
      action: { kind: "route", outbound: "proxy" },
    };
    updateRouting({ rules: [...r.rules, rule] });
  };

  const onAddRuleSet = (rs: CustomRuleSet) => {
    if (r.rule_sets.some((x) => x.tag === rs.tag)) {
      alert(`Rule-set "${rs.tag}" already exists`);
      return;
    }
    updateRouting({ rule_sets: [...r.rule_sets, rs] });
  };

  const onResetRouting = () => {
    if (r.rules.length > 0 || r.rule_sets.length > 0 || r.vpn_processes.length > 0 || r.direct_processes.length > 0) {
      if (!confirm("Reset all routing rules, rule-sets and process pickers? Tunnel/DNS/port settings are not affected.")) return;
    }
    updateRouting(DEFAULT_ROUTING);
  };

  const onCopyJson = async () => {
    try {
      await navigator.clipboard.writeText(JSON.stringify(jsonPreview, null, 2));
      setJsonCopied(true);
      setTimeout(() => setJsonCopied(false), 1500);
    } catch { /* ignore */ }
  };

  // Get all unique rule-set tags used in any rule's matchers (for warnings).
  const missingRuleSetTags = useMemo(() => {
    const used = new Set<string>();
    for (const rule of r.rules) {
      if (rule.enabled) {
        for (const tag of rule.matchers.rule_set ?? []) used.add(tag);
      }
    }
    const have = new Set(r.rule_sets.filter((x) => x.enabled).map((x) => x.tag));
    return Array.from(used).filter((t) => !have.has(t));
  }, [r.rules, r.rule_sets]);

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-start justify-between gap-3 flex-wrap">
        <div>
          <h2 className="text-lg font-semibold text-foreground">Routing</h2>
          <p className="text-sm text-muted-foreground mt-0.5">
            Pick which programs go through the VPN. Everything else goes direct.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="ghost" size="sm" onClick={onResetRouting} title="Reset routing">
            <RotateCcw size={14} className="mr-1" />
            Reset
          </Button>
        </div>
      </div>

      {/* Simple UX: two process-picker cards. */}
      <ProcessPickerCard
        icon={<Shield size={16} className="text-primary" />}
        title="Apps via VPN"
        description="Traffic from these programs goes through the VPN. Everything else stays direct."
        accent="vpn"
        selected={r.vpn_processes}
        onChange={(next) => updateRouting({ vpn_processes: next })}
      />
      <ProcessPickerCard
        icon={<ShieldOff size={16} className="text-muted-foreground" />}
        title="Apps direct"
        description="Always bypass the VPN, even if a rule-set or final outbound would otherwise route them via proxy. For most users, leave empty."
        accent="direct"
        selected={r.direct_processes}
        onChange={(next) => updateRouting({ direct_processes: next })}
      />

      {/* Advanced — collapsed by default. Houses the full rule editor
          and JSON preview. Non-tech-savvy users never see this. */}
      <details
        className="rounded-md border border-border bg-card/30 open:bg-card/40 transition"
        open={advancedOpen}
        onToggle={(e) => setAdvancedOpen((e.target as HTMLDetailsElement).open)}
      >
        <summary className="cursor-pointer select-none px-3 py-2 text-sm font-medium text-foreground hover:text-primary flex items-center gap-2 list-none [&::-webkit-details-marker]:hidden">
          <span className="text-xs text-muted-foreground">{advancedOpen ? "▾" : "▸"}</span>
          Advanced
          {r.rules.length > 0 && (
            <span className="text-[10px] text-muted-foreground font-normal">
              — {r.rules.length} rule{r.rules.length === 1 ? "" : "s"}, {r.rule_sets.length} rule-set{r.rule_sets.length === 1 ? "" : "s"}
            </span>
          )}
        </summary>

        <div className="px-3 pb-3 pt-1 space-y-4 border-t border-border">
          {/* General settings — sniff, final, auto_detect_interface */}
          <div className="rounded-md border border-border bg-background/30 p-4">
            <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
              <label className="flex items-center gap-2 text-sm text-foreground">
                <input
                  type="checkbox"
                  checked={r.sniff}
                  onChange={(e) => updateRouting({ sniff: e.target.checked })}
                  className="rounded border-input bg-background"
                />
                Sniff protocol (HTTP/TLS/QUIC)
              </label>
              <label className="flex items-center gap-2 text-sm text-foreground">
                <input
                  type="checkbox"
                  checked={r.auto_detect_interface}
                  onChange={(e) => updateRouting({ auto_detect_interface: e.target.checked })}
                  className="rounded border-input bg-background"
                />
                Auto-detect interface
              </label>
              <div>
                <label className="block text-xs text-muted-foreground mb-1">Final outbound</label>
                <select
                  value={r.final_outbound}
                  onChange={(e) => updateRouting({ final_outbound: e.target.value })}
                  className="w-full rounded-md bg-background border border-input px-2 py-1 text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
                >
                  <option value="direct">direct (default for the simple UX)</option>
                  <option value="proxy">proxy (selector)</option>
                  <option value="auto">auto (urltest)</option>
                  <option value="block">block</option>
                  {profiles
                    .filter((o) => o.protocol !== "unsupported")
                    .map((p) => (
                      <option key={p.tag} value={p.tag}>
                        {p.tag}
                      </option>
                    ))}
                </select>
              </div>
            </div>
          </div>

          {/* Warning: rules reference missing rule-sets */}
          {missingRuleSetTags.length > 0 && (
            <div className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive-foreground">
              ⚠ Rules reference rule-set{missingRuleSetTags.length > 1 ? "s" : ""} that aren&apos;t enabled:{" "}
              {missingRuleSetTags.map((t) => (
                <code key={t} className="mx-0.5 font-mono bg-destructive/20 text-destructive-foreground rounded px-1">
                  {t}
                </code>
              ))}
              . Add them from the picker below or they&apos;ll be silently dropped.
            </div>
          )}

          {/* Main: rule list */}
          <div>
            <h3 className="text-sm font-medium text-foreground mb-2">Custom rules ({r.rules.length})</h3>
            <RuleList
              rules={r.rules}
              outbounds={profiles.filter(
                (o): o is Exclude<typeof o, { protocol: "unsupported" }> =>
                  o.protocol !== "unsupported",
              )}
              onChange={(rules) => updateRouting({ rules })}
              onAdd={onAddRule}
            />
          </div>

          {/* Rule-sets */}
          <RuleSetsPanel
            ruleSets={r.rule_sets}
            onChange={(rule_sets) => updateRouting({ rule_sets })}
          />

          {/* Preset library */}
          <PresetPicker onAddRule={onAddRule} onAddRuleSet={onAddRuleSet} />

          {/* JSON preview (toggled by its own button) */}
          <div className="flex items-center justify-end">
            <Button variant="ghost" size="sm" onClick={() => setJsonOpen(!jsonOpen)}>
              {jsonOpen ? "Hide JSON" : "Show JSON"}
            </Button>
          </div>
          {jsonOpen && (
            <div className="rounded-md border border-border bg-card/30 p-3">
              <div className="flex items-center justify-between mb-2">
                <span className="text-xs uppercase tracking-wide text-muted-foreground">
                  Generated sing-box route block (informational)
                </span>
                <Button variant="ghost" size="sm" onClick={onCopyJson} title="Copy JSON">
                  <Copy size={12} className="mr-1" />
                  {jsonCopied ? "Copied" : "Copy"}
                </Button>
              </div>
              <pre className="text-xs text-foreground/80 overflow-x-auto whitespace-pre-wrap font-mono">
                {JSON.stringify(jsonPreview, null, 2)}
              </pre>
            </div>
          )}
        </div>
      </details>
    </div>
  );
}

// ---- Sub-components -----------------------------------------------

/** A "card" combining a header, a list of selected processes as
 *  removable chips, and a collapsible ProcessPicker below. This is
 *  the only thing a non-tech-savvy user ever sees on the Routing
 *  tab — both for "Apps via VPN" and "Apps direct". */
function ProcessPickerCard({
  icon,
  title,
  description,
  accent,
  selected,
  onChange,
}: {
  icon: React.ReactNode;
  title: string;
  description: string;
  accent: "vpn" | "direct";
  selected: string[];
  onChange: (next: string[]) => void;
}) {
  return (
    <section className="rounded-md border border-border bg-card/30 p-4 space-y-3">
      <div className="flex items-start gap-2.5">
        <div className="mt-0.5">{icon}</div>
        <div className="min-w-0">
          <h3 className="text-sm font-medium text-foreground">{title}</h3>
          <p className="text-xs text-muted-foreground mt-0.5">{description}</p>
        </div>
      </div>

      {/* Selected chips. Empty state shows a tiny hint. */}
      {selected.length === 0 ? (
        <div className="text-xs text-muted-foreground italic">
          No programs picked. {accent === "vpn"
            ? "Anything you add here will go through the VPN."
            : "Anything you add here will always go direct, ignoring the proxy."}
        </div>
      ) : (
        <div className="flex flex-wrap gap-1.5">
          {selected.map((name) => (
            <button
              key={name}
              type="button"
              onClick={() => onChange(selected.filter((n) => n !== name))}
              className={cn(
                "inline-flex items-center gap-1 rounded-full border px-2.5 py-0.5 text-xs transition",
                "hover:opacity-80",
                accent === "vpn"
                  ? "border-primary/40 bg-primary/10 text-primary"
                  : "border-muted-foreground/40 bg-muted text-foreground",
              )}
              title={`Remove ${name}`}
            >
              <span className="font-mono">{name}</span>
              <X size={11} className="opacity-70" />
            </button>
          ))}
        </div>
      )}

      {/* The reusable ProcessPicker. The button "Pick from running…"
          stays inside it; the chip area above is the user's quick
          way to see and remove selections. */}
      <ProcessPicker selected={selected} onChange={onChange} />
    </section>
  );
}

// ---- Default used by "Reset" ---------------------------------------

const DEFAULT_ROUTING: RoutingOptions = {
  rules: [],
  rule_sets: [],
  vpn_processes: [],
  direct_processes: [],
  sniff: true,
  final_outbound: "direct",
  auto_detect_interface: true,
  default_domain_resolver: "local",
};

/** Build the sing-box `route` JSON view from a RoutingOptions. */
function buildJsonPreview(r: RoutingOptions) {
  const out: Record<string, unknown> = {};
  const rules: Record<string, unknown>[] = [];

  // Hard-coded DNS-bypass + optional sniff (mirrors Rust).
  rules.push({ network: "dns", action: "direct" });
  if (r.sniff) {
    rules.push({ action: "sniff" });
  }
  // Process-picker rules (the "simple" UX). Direct first, VPN second.
  if (r.direct_processes.length > 0) {
    rules.push({
      process_name: r.direct_processes,
      action: "route",
      outbound: "direct",
    });
  }
  if (r.vpn_processes.length > 0) {
    rules.push({
      process_name: r.vpn_processes,
      action: "route",
      outbound: "auto",
    });
  }
  for (const rule of r.rules) {
    if (!rule.enabled) continue;
    const cleaned: Record<string, unknown> = {};
    // Strip empty arrays / falsy optionals so the preview is readable.
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
      rules.push({ ...cleaned, action: "resolve" });
    }
  }
  out.rules = rules;
  const ruleSets = r.rule_sets
    .filter((rs) => rs.enabled)
    .map((rs) => {
      const o: Record<string, unknown> = {
        tag: rs.tag,
        type: rs.type,
      };
      if (rs.format) o.format = rs.format;
      if (rs.type === "remote" && rs.url) o.url = rs.url;
      if (rs.type === "local" && rs.path) o.path = rs.path;
      if (rs.update_interval) o.update_interval = rs.update_interval;
      return o;
    });
  if (ruleSets.length > 0) out.rule_set = ruleSets;
  out.final = r.final_outbound;
  out.auto_detect_interface = r.auto_detect_interface;
  out.default_domain_resolver = r.default_domain_resolver;
  return out;
}
