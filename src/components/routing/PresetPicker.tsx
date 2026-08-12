// PresetPicker — quick insert for starter rules (Bypass LAN, Block Ads, etc).
//
// Two sections: "Starter rules" (RULE_PRESETS, become CustomRule in the
// rule list) and "Rule-sets" (RULE_SET_PRESETS, become CustomRuleSet in
// the rule-set list). Source toggle at the top filters which rule-sets
// are shown.

import { useState } from "react";
import { Plus } from "lucide-react";
import { Badge } from "../Badge";
import { Button } from "../Button";
import {
  RULE_PRESETS,
  RULE_SET_PRESETS,
  newRuleId,
  presetToRuleSet,
  type PresetSource,
  type RuleSetPreset,
} from "@/lib/presets";
import type { CustomRule, CustomRuleSet } from "@/lib/types";

interface Props {
  onAddRule: (rule: CustomRule) => void;
  onAddRuleSet: (rs: CustomRuleSet) => void;
}

export function PresetPicker({ onAddRule, onAddRuleSet }: Props) {
  const [source, setSource] = useState<PresetSource | "all">("all");
  const filtered = RULE_SET_PRESETS.filter(
    (p) => source === "all" || p.source === source,
  );

  return (
    <div className="rounded-md border border-slate-800 bg-slate-900/30 p-4 space-y-4">
      <div>
        <h3 className="text-sm font-medium text-slate-200">Starter rules</h3>
        <p className="text-xs text-slate-500 mt-0.5">
          One-click inserts for common routing rules.
        </p>
        <div className="mt-3 grid grid-cols-1 sm:grid-cols-2 gap-2">
          {RULE_PRESETS.map((p) => (
            <button
              key={p.id}
              type="button"
              onClick={() => onAddRule({ id: newRuleId(), ...p.build() })}
              className="text-left rounded-md border border-slate-800 bg-slate-950/60 hover:border-sky-500/50 hover:bg-slate-900 transition px-3 py-2 group"
            >
              <div className="flex items-center gap-2">
                <Plus size={14} className="text-slate-500 group-hover:text-sky-400" />
                <span className="text-sm text-slate-100">{p.label}</span>
              </div>
              <div className="text-xs text-slate-500 mt-0.5 ml-5 line-clamp-2">
                {p.description}
              </div>
            </button>
          ))}
        </div>
      </div>

      <div className="border-t border-slate-800 pt-4">
        <div className="flex items-center justify-between gap-2 mb-2">
          <div>
            <h3 className="text-sm font-medium text-slate-200">Rule-set library</h3>
            <p className="text-xs text-slate-500 mt-0.5">
              Pre-built rule-sets (Loyalsoldier / meta-rules-dat). Use the
              rule-set tag in any rule's <em>Rule-set</em> field.
            </p>
          </div>
          <div className="flex items-center gap-1 rounded-md border border-slate-800 bg-slate-950 p-0.5 text-xs">
            {(["all", "loyalsoldier", "meta"] as const).map((s) => (
              <button
                key={s}
                type="button"
                onClick={() => setSource(s)}
                className={
                  "px-2 py-1 rounded transition " +
                  (source === s
                    ? "bg-sky-500 text-white"
                    : "text-slate-400 hover:text-slate-200")
                }
              >
                {s}
              </button>
            ))}
          </div>
        </div>
        <div className="space-y-1.5 max-h-64 overflow-y-auto pr-1">
          {filtered.map((p) => (
            <RuleSetPresetRow
              key={p.tag}
              preset={p}
              onAdd={() => onAddRuleSet(presetToRuleSet(p))}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

function RuleSetPresetRow({
  preset,
  onAdd,
}: {
  preset: RuleSetPreset;
  onAdd: () => void;
}) {
  return (
    <div className="flex items-center gap-2 rounded border border-slate-800 bg-slate-950/40 px-2.5 py-1.5">
      <code className="text-xs text-sky-300 font-mono">{preset.tag}</code>
      <span className="text-sm text-slate-200 truncate flex-1">{preset.label}</span>
      <Badge variant="default">{preset.source}</Badge>
      <Button variant="ghost" size="sm" onClick={onAdd} title="Add rule-set">
        <Plus size={12} />
      </Button>
    </div>
  );
}
