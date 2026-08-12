// RuleEditor — inline editor for one CustomRule.
//
// When a `RuleRow` is expanded, this component renders BELOW the row
// with a flat list of matcher chips + an action selector. Saving is
// implicit — every keystroke calls `onChange`.
//
// Design goals:
//   - 95% of users only need: domain / domain_suffix / ip_cidr / port /
//     rule_set / action=route|reject. Those are always visible.
//   - Advanced matchers (process_*, regex, source_*, auth_user, ...) are
//     hidden under a collapsible "Advanced" disclosure.
//   - No nested forms. One field type per line. Add/remove via inline buttons.

import { useState } from "react";
import { ChevronDown, ChevronUp, Plus, Trash2 } from "lucide-react";
import { Button } from "../Button";
import type { CustomRule, Outbound, RuleAction, RuleMatchers } from "@/lib/types";
import { cn } from "@/lib/utils";

interface Props {
  rule: CustomRule;
  /** All available outbound tags (for the action dropdown). */
  outbounds: Outbound[];
  onChange: (next: CustomRule) => void;
}

type ArrayField = keyof Pick<
  RuleMatchers,
  | "domain"
  | "domain_suffix"
  | "domain_keyword"
  | "domain_regex"
  | "ip_cidr"
  | "source_ip_cidr"
  | "port"
  | "port_range"
  | "source_port"
  | "source_port_range"
  | "process_name"
  | "process_path"
  | "protocol"
  | "rule_set"
  | "inbound"
>;

// Pre-defined chips for fields where the set of values is fixed.
const NETWORK_OPTIONS = ["tcp", "udp", "icmp"] as const;
const PROTOCOL_OPTIONS = ["http", "tls", "quic", "dns", "stun", "ntp", "ssh", "rdp", "mongodb", "postgresql"] as const;
const ACTION_KINDS: Array<RuleAction["kind"]> = ["route", "reject", "hijack-dns", "sniff", "resolve"];

export function RuleEditor({ rule, outbounds, onChange }: Props) {
  const [showAdvanced, setShowAdvanced] = useState(false);
  const m = rule.matchers;
  const a = rule.action;

  const setAction = (next: RuleAction) => onChange({ ...rule, action: next });
  const setLabel = (label: string) => onChange({ ...rule, label });

  /** Update a string[] matcher (add / edit / remove). */
  const updateArray = (field: ArrayField, values: string[] | number[] | undefined) => {
    const next: RuleMatchers = { ...m };
    if (values === undefined || (Array.isArray(values) && values.length === 0)) {
      delete (next as Record<string, unknown>)[field as string];
    } else {
      (next as Record<string, unknown>)[field as string] = values;
    }
    onChange({ ...rule, matchers: next });
  };

  const toggleInArray = (field: ArrayField, value: string) => {
    const cur = (m[field] as string[] | undefined) ?? [];
    const next = cur.includes(value) ? cur.filter((v) => v !== value) : [...cur, value];
    updateArray(field, next);
  };

  const toggleInPortArray = (field: "port" | "source_port", value: number) => {
    const cur = (m[field] as number[] | undefined) ?? [];
    const next = cur.includes(value) ? cur.filter((v) => v !== value) : [...cur, value];
    updateArray(field, next);
  };

  return (
    <div className="rounded-md border border-slate-800 bg-slate-900/60 p-4 space-y-4">
      {/* Label */}
      <div>
        <label className="block text-xs uppercase tracking-wide text-slate-400 mb-1">
          Label
        </label>
        <input
          type="text"
          value={rule.label ?? ""}
          onChange={(e) => setLabel(e.target.value)}
          placeholder="e.g. Work split-tunnel"
          className="w-full rounded bg-slate-950 border border-slate-800 px-3 py-1.5 text-sm text-slate-100 placeholder:text-slate-600 focus:outline-none focus:ring-1 focus:ring-sky-500"
        />
      </div>

      {/* Action */}
      <div>
        <label className="block text-xs uppercase tracking-wide text-slate-400 mb-1">
          Action
        </label>
        <div className="flex flex-wrap items-center gap-2">
          {ACTION_KINDS.map((kind) => (
            <button
              key={kind}
              type="button"
              onClick={() => {
                if (kind === "route") setAction({ kind, outbound: "proxy" });
                else if (kind === "reject") setAction({ kind });
                else if (kind === "hijack-dns") setAction({ kind });
                else if (kind === "sniff") setAction({ kind });
                else if (kind === "resolve") setAction({ kind });
              }}
              className={cn(
                "rounded px-2.5 py-1 text-xs font-medium border transition",
                a.kind === kind
                  ? "bg-sky-500 text-white border-sky-500"
                  : "bg-slate-950 text-slate-300 border-slate-800 hover:border-slate-700",
              )}
            >
              {kind}
            </button>
          ))}
        </div>
        {a.kind === "route" && (
          <div className="mt-2">
            <select
              value={a.outbound}
              onChange={(e) => setAction({ kind: "route", outbound: e.target.value })}
              className="w-full rounded bg-slate-950 border border-slate-800 px-3 py-1.5 text-sm text-slate-100 focus:outline-none focus:ring-1 focus:ring-sky-500"
            >
              <option value="proxy">proxy (selector)</option>
              <option value="auto">auto (urltest)</option>
              <option value="direct">direct</option>
              <option value="block">block</option>
              {outbounds
                .filter((o) => o.protocol !== "unsupported")
                .map((o) => (
                  <option key={o.tag} value={o.tag}>
                    {o.tag} ({o.protocol})
                  </option>
                ))}
            </select>
          </div>
        )}
      </div>

      {/* Matchers — main (always visible) */}
      <div className="space-y-3">
        <ChipField
          label="Domain (exact)"
          field="domain"
          values={m.domain}
          placeholder="example.com"
          onChange={(v) => updateArray("domain", v)}
        />
        <ChipField
          label="Domain suffix"
          field="domain_suffix"
          values={m.domain_suffix}
          placeholder=".example.com"
          onChange={(v) => updateArray("domain_suffix", v)}
        />
        <ChipField
          label="Domain keyword"
          field="domain_keyword"
          placeholder="substring"
          values={m.domain_keyword}
          onChange={(v) => updateArray("domain_keyword", v)}
        />
        <ChipField
          label="IP CIDR"
          field="ip_cidr"
          values={m.ip_cidr}
          placeholder="10.0.0.0/8"
          onChange={(v) => updateArray("ip_cidr", v)}
        />
        <PortField
          label="Port"
          field="port"
          values={m.port}
          onChange={(v) => updateArray("port", v)}
        />
        <ChipField
          label="Port range (e.g. 1000:2000)"
          field="port_range"
          values={m.port_range}
          placeholder="1000:2000"
          onChange={(v) => updateArray("port_range", v)}
        />
        <ChipsPicker
          label="Network"
          options={NETWORK_OPTIONS}
          values={(m.network as string[] | undefined) ?? []}
          onToggle={(v) => toggleInArray("network" as ArrayField, v)}
        />
        <ChipsPicker
          label="Protocol (sniffed)"
          options={PROTOCOL_OPTIONS}
          values={(m.protocol as string[] | undefined) ?? []}
          onToggle={(v) => toggleInArray("protocol" as ArrayField, v)}
        />
        <ChipField
          label="Rule-set (tag)"
          field="rule_set"
          values={m.rule_set}
          placeholder="geoip-cn"
          onChange={(v) => updateArray("rule_set", v)}
        />
        <div className="flex items-center gap-2">
          <input
            type="checkbox"
            id={`invert-${rule.id}`}
            checked={rule.invert === true}
            onChange={(e) =>
              onChange({ ...rule, invert: e.target.checked || undefined })
            }
            className="rounded border-slate-700 bg-slate-950"
          />
          <label htmlFor={`invert-${rule.id}`} className="text-sm text-slate-300 select-none">
            Invert (match everything <em>except</em> these conditions)
          </label>
        </div>
      </div>

      {/* Advanced — collapsed by default */}
      <div className="border-t border-slate-800 pt-3">
        <button
          type="button"
          onClick={() => setShowAdvanced(!showAdvanced)}
          className="flex items-center gap-1 text-xs text-slate-400 hover:text-slate-200"
        >
          {showAdvanced ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
          Advanced matchers
        </button>
        {showAdvanced && (
          <div className="mt-3 space-y-3">
            <ChipField
              label="Domain regex"
              field="domain_regex"
              values={m.domain_regex}
              placeholder="^stun\\..+"
              onChange={(v) => updateArray("domain_regex", v)}
            />
            <ChipField
              label="Source IP CIDR"
              field="source_ip_cidr"
              values={m.source_ip_cidr}
              placeholder="192.168.0.0/16"
              onChange={(v) => updateArray("source_ip_cidr", v)}
            />
            <PortField
              label="Source port"
              field="source_port"
              values={m.source_port}
              onChange={(v) => updateArray("source_port", v)}
            />
            <ChipField
              label="Process name (Win/Mac/Linux)"
              field="process_name"
              values={m.process_name}
              placeholder="chrome"
              onChange={(v) => updateArray("process_name", v)}
            />
            <ChipField
              label="Process path"
              field="process_path"
              values={m.process_path}
              placeholder="/usr/bin/curl"
              onChange={(v) => updateArray("process_path", v)}
            />
            <ChipField
              label="Inbound tag"
              field="inbound"
              values={m.inbound}
              placeholder="tun-in"
              onChange={(v) => updateArray("inbound", v)}
            />
            <div className="flex items-center gap-2">
              <input
                type="checkbox"
                id={`private-${rule.id}`}
                checked={m.ip_is_private === true}
                onChange={(e) =>
                  onChange({
                    ...rule,
                    matchers: e.target.checked
                      ? { ...m, ip_is_private: true }
                      : omitKey(m, "ip_is_private"),
                  })
                }
                className="rounded border-slate-700 bg-slate-950"
              />
              <label htmlFor={`private-${rule.id}`} className="text-sm text-slate-300 select-none">
                Match non-public (private) IPs
              </label>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

// ── Small sub-components ────────────────────────────────────────────────

interface ChipFieldProps {
  label: string;
  field: string;
  values: string[] | undefined;
  placeholder?: string;
  onChange: (next: string[] | undefined) => void;
}

function ChipField({ label, values, placeholder, onChange }: ChipFieldProps) {
  const [draft, setDraft] = useState("");
  const arr = values ?? [];

  const add = () => {
    const v = draft.trim();
    if (!v) return;
    if (arr.includes(v)) {
      setDraft("");
      return;
    }
    onChange([...arr, v]);
    setDraft("");
  };
  const remove = (v: string) => onChange(arr.filter((x) => x !== v));

  return (
    <div>
      <label className="block text-xs uppercase tracking-wide text-slate-400 mb-1">{label}</label>
      <div className="flex flex-wrap gap-1.5 items-center">
        {arr.map((v) => (
          <span
            key={v}
            className="inline-flex items-center gap-1 rounded-full bg-sky-500/15 text-sky-200 px-2 py-0.5 text-xs"
          >
            {v}
            <button
              type="button"
              onClick={() => remove(v)}
              className="text-sky-300/70 hover:text-sky-100"
              aria-label={`Remove ${v}`}
            >
              ×
            </button>
          </span>
        ))}
        <div className="flex items-center gap-1">
          <input
            type="text"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === ",") {
                e.preventDefault();
                add();
              } else if (e.key === "Backspace" && draft === "" && arr.length > 0) {
                remove(arr[arr.length - 1]);
              }
            }}
            placeholder={placeholder}
            className="rounded bg-slate-950 border border-slate-800 px-2 py-0.5 text-xs text-slate-100 placeholder:text-slate-600 focus:outline-none focus:ring-1 focus:ring-sky-500 w-32"
          />
          <Button variant="ghost" size="sm" onClick={add} title="Add">
            <Plus size={12} />
          </Button>
        </div>
      </div>
    </div>
  );
}

interface PortFieldProps {
  label: string;
  field: string;
  values: number[] | undefined;
  onChange: (next: number[] | undefined) => void;
}

function PortField({ label, values, onChange }: PortFieldProps) {
  const [draft, setDraft] = useState("");
  const arr = values ?? [];
  const add = () => {
    const n = parseInt(draft.trim(), 10);
    if (Number.isNaN(n) || n < 1 || n > 65535) return;
    if (arr.includes(n)) { setDraft(""); return; }
    onChange([...arr, n]);
    setDraft("");
  };
  const remove = (n: number) => onChange(arr.filter((x) => x !== n));
  return (
    <div>
      <label className="block text-xs uppercase tracking-wide text-slate-400 mb-1">{label}</label>
      <div className="flex flex-wrap gap-1.5 items-center">
        {arr.map((n) => (
          <span
            key={n}
            className="inline-flex items-center gap-1 rounded-full bg-emerald-500/15 text-emerald-200 px-2 py-0.5 text-xs"
          >
            {n}
            <button
              type="button"
              onClick={() => remove(n)}
              className="text-emerald-300/70 hover:text-emerald-100"
              aria-label={`Remove port ${n}`}
            >
              ×
            </button>
          </span>
        ))}
        <div className="flex items-center gap-1">
          <input
            type="number"
            min={1}
            max={65535}
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") { e.preventDefault(); add(); }
            }}
            placeholder="443"
            className="rounded bg-slate-950 border border-slate-800 px-2 py-0.5 text-xs text-slate-100 placeholder:text-slate-600 focus:outline-none focus:ring-1 focus:ring-sky-500 w-20"
          />
          <Button variant="ghost" size="sm" onClick={add} title="Add">
            <Plus size={12} />
          </Button>
        </div>
      </div>
    </div>
  );
}

interface ChipsPickerProps {
  label: string;
  options: readonly string[];
  values: string[];
  onToggle: (value: string) => void;
}

function ChipsPicker({ label, options, values, onToggle }: ChipsPickerProps) {
  return (
    <div>
      <label className="block text-xs uppercase tracking-wide text-slate-400 mb-1">{label}</label>
      <div className="flex flex-wrap gap-1.5">
        {options.map((opt) => {
          const active = values.includes(opt);
          return (
            <button
              key={opt}
              type="button"
              onClick={() => onToggle(opt)}
              className={cn(
                "rounded-full px-2.5 py-0.5 text-xs border transition",
                active
                  ? "bg-violet-500 text-white border-violet-500"
                  : "bg-slate-950 text-slate-300 border-slate-800 hover:border-slate-700",
              )}
            >
              {opt}
            </button>
          );
        })}
      </div>
    </div>
  );
}

function omitKey<T extends object, K extends keyof T>(o: T, k: K): Omit<T, K> {
  const copy = { ...o };
  delete copy[k];
  return copy;
}
