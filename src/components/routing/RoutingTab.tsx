// RoutingTab — main container for the new "Routing" page.
//
// Lifts the routing options from `GeneratorSettings.routing` and routes
// changes back via `onSettingsChange`. Owns no state of its own except
// derived UI bits (e.g. the JSON preview link).

import { useMemo, useState } from "react";
import { Copy, RotateCcw } from "lucide-react";
import { Button } from "../Button";
import { RuleList } from "./RuleList";
import { PresetPicker } from "./PresetPicker";
import { RuleSetsPanel } from "./RuleSetsPanel";
import { newRuleId } from "@/lib/presets";
import type { CustomRule, CustomRuleSet, GeneratorSettings, Outbound, RoutingOptions } from "@/lib/types";

interface Props {
  profiles: Outbound[];
  settings: GeneratorSettings;
  onSettingsChange: (next: GeneratorSettings) => void;
}

const DEFAULT_ROUTING: RoutingOptions = {
  rules: [],
  rule_sets: [],
  sniff: true,
  final_outbound: "proxy",
  auto_detect_interface: true,
  default_domain_resolver: "local",
};

export function RoutingTab({ profiles, settings, onSettingsChange }: Props) {
  const r = settings.routing;
  const updateRouting = (patch: Partial<RoutingOptions>) =>
    onSettingsChange({ ...settings, routing: { ...r, ...patch } });

  const [jsonOpen, setJsonOpen] = useState(false);
  const [jsonCopied, setJsonCopied] = useState(false);

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
    if (r.rules.length > 0 || r.rule_sets.length > 0) {
      if (!confirm("Reset all routing rules and rule-sets? Tunnel/DNS/port settings are not affected.")) return;
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
          <h2 className="text-lg font-semibold text-slate-100">Routing</h2>
          <p className="text-sm text-slate-400 mt-0.5">
            Order rules from most specific to most generic. First match wins.
            Drag to reorder, click to expand.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="ghost" size="sm" onClick={() => setJsonOpen(!jsonOpen)}>
            {jsonOpen ? "Hide JSON" : "Show JSON"}
          </Button>
          <Button variant="ghost" size="sm" onClick={onResetRouting} title="Reset routing">
            <RotateCcw size={14} className="mr-1" />
            Reset
          </Button>
        </div>
      </div>

      {/* General settings — sniff, final, auto_detect_interface */}
      <div className="rounded-md border border-slate-800 bg-slate-900/30 p-4">
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
          <label className="flex items-center gap-2 text-sm text-slate-200">
            <input
              type="checkbox"
              checked={r.sniff}
              onChange={(e) => updateRouting({ sniff: e.target.checked })}
              className="rounded border-slate-700 bg-slate-950"
            />
            Sniff protocol (HTTP/TLS/QUIC)
          </label>
          <label className="flex items-center gap-2 text-sm text-slate-200">
            <input
              type="checkbox"
              checked={r.auto_detect_interface}
              onChange={(e) => updateRouting({ auto_detect_interface: e.target.checked })}
              className="rounded border-slate-700 bg-slate-950"
            />
            Auto-detect interface
          </label>
          <div>
            <label className="block text-xs text-slate-400 mb-1">Final outbound</label>
            <select
              value={r.final_outbound}
              onChange={(e) => updateRouting({ final_outbound: e.target.value })}
              className="w-full rounded bg-slate-950 border border-slate-800 px-2 py-1 text-sm text-slate-100 focus:outline-none focus:ring-1 focus:ring-sky-500"
            >
              <option value="proxy">proxy (selector)</option>
              <option value="auto">auto (urltest)</option>
              <option value="direct">direct</option>
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
        <div className="rounded-md border border-amber-700/40 bg-amber-900/15 px-3 py-2 text-sm text-amber-200">
          ⚠ Rules reference rule-set{missingRuleSetTags.length > 1 ? "s" : ""} that aren&apos;t enabled:{" "}
          {missingRuleSetTags.map((t) => (
            <code key={t} className="mx-0.5 font-mono bg-amber-950/40 rounded px-1">
              {t}
            </code>
          ))}
          . Add them from the picker below or they&apos;ll be silently dropped.
        </div>
      )}

      {/* Main: rule list */}
      <div>
        <h3 className="text-sm font-medium text-slate-200 mb-2">Custom rules ({r.rules.length})</h3>
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

      {/* JSON preview */}
      {jsonOpen && (
        <div className="rounded-md border border-slate-800 bg-slate-950 p-3">
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs uppercase tracking-wide text-slate-400">
              Generated sing-box route block (informational)
            </span>
            <Button variant="ghost" size="sm" onClick={onCopyJson} title="Copy JSON">
              <Copy size={12} className="mr-1" />
              {jsonCopied ? "Copied" : "Copy"}
            </Button>
          </div>
          <pre className="text-xs text-slate-300 overflow-x-auto whitespace-pre-wrap">
            {JSON.stringify(jsonPreview, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}

/** Build the sing-box `route` JSON view from a RoutingOptions. */
function buildJsonPreview(r: RoutingOptions) {
  const out: Record<string, unknown> = {};
  const rules: Record<string, unknown>[] = [];

  if (r.sniff) {
    rules.push({ action: "sniff" });
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
