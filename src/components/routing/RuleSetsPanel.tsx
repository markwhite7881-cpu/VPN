// RuleSetsPanel — manage the list of CustomRuleSet (external Loyalsoldier /
// meta-rules-dat / user-added URLs).
//
// Mostly CRUD with chips. Inline editor for tag + URL + format.

import { Plus, Trash2 } from "lucide-react";
import { Badge } from "../Badge";
import { Button } from "../Button";
import type { CustomRuleSet } from "@/lib/types";

interface Props {
  ruleSets: CustomRuleSet[];
  onChange: (next: CustomRuleSet[]) => void;
}

export function RuleSetsPanel({ ruleSets, onChange }: Props) {
  const update = (tag: string, patch: Partial<CustomRuleSet>) => {
    onChange(ruleSets.map((rs) => (rs.tag === tag ? { ...rs, ...patch } : rs)));
  };
  const remove = (tag: string) => onChange(ruleSets.filter((rs) => rs.tag !== tag));
  const add = () => {
    const tag = prompt("Tag for new rule-set (a-z 0-9 - _ only):");
    if (!tag) return;
    if (!/^[a-z0-9_-]+$/.test(tag)) {
      alert("Tag must be a-z, 0-9, '-' or '_'");
      return;
    }
    if (ruleSets.some((rs) => rs.tag === tag)) {
      alert(`Rule-set "${tag}" already exists`);
      return;
    }
    const url = prompt("URL to .srs or .json rule-set:");
    if (!url) return;
    onChange([
      ...ruleSets,
      {
        tag,
        type: "remote",
        format: url.endsWith(".json") ? "source" : "binary",
        url,
        update_interval: "1d",
        enabled: true,
      },
    ]);
  };

  return (
    <div className="rounded-md border border-slate-800 bg-slate-900/30 p-4 space-y-3">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-sm font-medium text-slate-200">Rule-sets</h3>
          <p className="text-xs text-slate-500 mt-0.5">
            External rule-sets referenced by rules (Loyalsoldier, meta-rules-dat, custom).
          </p>
        </div>
        <Button variant="outline" size="sm" onClick={add}>
          <Plus size={12} className="mr-1" />
          Add
        </Button>
      </div>

      {ruleSets.length === 0 && (
        <div className="rounded border border-dashed border-slate-800 bg-slate-950/40 p-3 text-center text-xs text-slate-500">
          No rule-sets. Use the picker above to add Loyalsoldier or meta-rules-dat presets.
        </div>
      )}

      <div className="space-y-1.5">
        {ruleSets.map((rs) => (
          <div
            key={rs.tag}
            className={
              "rounded border border-slate-800 bg-slate-950/40 px-3 py-2 " +
              (rs.enabled ? "" : "opacity-60")
            }
          >
            <div className="flex items-center gap-2 flex-wrap">
              <input
                type="checkbox"
                checked={rs.enabled}
                onChange={(e) => update(rs.tag, { enabled: e.target.checked })}
                title="Enable / disable"
              />
              <code className="text-xs text-sky-300 font-mono">{rs.tag}</code>
              <Badge variant="default">{rs.type}</Badge>
              {rs.format && <Badge variant="default">{rs.format}</Badge>}
              {rs.update_interval && <Badge variant="default">↻ {rs.update_interval}</Badge>}
              <Button
                variant="ghost"
                size="sm"
                onClick={() => remove(rs.tag)}
                title="Remove rule-set"
                className="ml-auto"
              >
                <Trash2 size={12} />
              </Button>
            </div>
            {rs.type === "remote" && (
              <div className="mt-2">
                <input
                  type="text"
                  value={rs.url ?? ""}
                  onChange={(e) => update(rs.tag, { url: e.target.value })}
                  placeholder="https://…/geoip-cn.srs"
                  className="w-full rounded bg-slate-950 border border-slate-800 px-2 py-1 text-xs text-slate-200 placeholder:text-slate-600 focus:outline-none focus:ring-1 focus:ring-sky-500 font-mono"
                />
              </div>
            )}
            {rs.type === "local" && (
              <div className="mt-2">
                <input
                  type="text"
                  value={rs.path ?? ""}
                  onChange={(e) => update(rs.tag, { path: e.target.value })}
                  placeholder="C:\rules\my.srs"
                  className="w-full rounded bg-slate-950 border border-slate-800 px-2 py-1 text-xs text-slate-200 placeholder:text-slate-600 focus:outline-none focus:ring-1 focus:ring-sky-500 font-mono"
                />
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
