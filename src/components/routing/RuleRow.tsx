// RuleRow — one rule in the sortable list.
//
// Compact (collapsed): drag handle, status pill, label, action summary,
// expand button, enable toggle, delete.
// Expanded: shows RuleEditor below.
//
// Used inside RuleList's <SortableContext>; receives drag attributes
// from `useSortable` (the parent wires that up via the `sortable` prop).

import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { ChevronDown, ChevronUp, GripVertical, Power, Trash2 } from "lucide-react";
import { Badge } from "../Badge";
import { Button } from "../Button";
import { RuleEditor } from "./RuleEditor";
import type { CustomRule, Outbound } from "@/lib/types";
import { cn } from "@/lib/utils";

interface Props {
  rule: CustomRule;
  outbounds: Outbound[];
  expanded: boolean;
  onToggleExpanded: () => void;
  onChange: (next: CustomRule) => void;
  onDelete: () => void;
}

export function RuleRow({
  rule,
  outbounds,
  expanded,
  onToggleExpanded,
  onChange,
  onDelete,
}: Props) {
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: rule.id });

  const style: React.CSSProperties = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.5 : 1,
  };

  // Build a one-line "summary" of matchers + action for the collapsed view.
  const m = rule.matchers;
  const matcherSummary: string[] = [];
  if (m.domain?.length) matcherSummary.push(`domain × ${m.domain.length}`);
  if (m.domain_suffix?.length) matcherSummary.push(`suffix × ${m.domain_suffix.length}`);
  if (m.domain_keyword?.length) matcherSummary.push(`keyword × ${m.domain_keyword.length}`);
  if (m.ip_cidr?.length) matcherSummary.push(`ip_cidr × ${m.ip_cidr.length}`);
  if (m.port?.length) matcherSummary.push(`port × ${m.port.length}`);
  if (m.port_range?.length) matcherSummary.push(`port_range × ${m.port_range.length}`);
  if (m.network?.length) matcherSummary.push(`net: ${m.network.join("|")}`);
  if (m.protocol?.length) matcherSummary.push(`proto: ${m.protocol.join("|")}`);
  if (m.rule_set?.length) matcherSummary.push(`rs: ${m.rule_set.join(",")}`);
  if (m.process_name?.length) matcherSummary.push(`proc × ${m.process_name.length}`);
  if (m.ip_version === 6) matcherSummary.push("ipv6");
  if (m.ip_is_private) matcherSummary.push("private");
  if (rule.invert) matcherSummary.push("inverted");

  const actionSummary =
    rule.action.kind === "route"
      ? `→ ${rule.action.outbound}`
      : rule.action.kind === "reject"
      ? "✕ reject"
      : rule.action.kind === "hijack-dns"
      ? "→ DNS"
      : `(${rule.action.kind})`;

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={cn(
        "rounded-md border bg-card/40 transition",
        rule.enabled ? "border-border" : "border-border/50 opacity-60",
        isDragging && "shadow-lg ring-1 ring-foreground/20",
      )}
    >
      {/* Collapsed bar */}
      <div className="flex items-center gap-2 px-3 py-2">
        <button
          type="button"
          {...attributes}
          {...listeners}
          className="text-muted-foreground hover:text-foreground/80 cursor-grab active:cursor-grabbing"
          aria-label="Drag to reorder"
        >
          <GripVertical size={16} />
        </button>

        <button
          type="button"
          onClick={() => onChange({ ...rule, enabled: !rule.enabled })}
          title={rule.enabled ? "Disable rule" : "Enable rule"}
          className={cn(
            "p-1 rounded transition",
            rule.enabled
              ? "text-foreground hover:text-foreground/80"
              : "text-muted-foreground/60 hover:text-muted-foreground",
          )}
        >
          <Power size={14} />
        </button>

        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 flex-wrap">
            <span className="text-sm text-foreground truncate">
              {rule.label || "(unnamed rule)"}
            </span>
            <Badge
              variant={
                rule.action.kind === "reject"
                  ? "destructive"
                  : rule.action.kind === "hijack-dns"
                  ? "secondary"
                  : "default"
              }
            >
              {actionSummary}
            </Badge>
          </div>
          {matcherSummary.length > 0 && (
            <div className="text-xs text-muted-foreground mt-0.5 truncate">
              {matcherSummary.join(" · ")}
            </div>
          )}
        </div>

        <Button
          variant="ghost"
          size="sm"
          onClick={onToggleExpanded}
          title={expanded ? "Collapse" : "Expand"}
        >
          {expanded ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
        </Button>

        <Button
          variant="ghost"
          size="sm"
          onClick={onDelete}
          title="Delete rule"
        >
          <Trash2 size={14} />
        </Button>
      </div>

      {/* Inline editor (expanded) */}
      {expanded && (
        <div className="px-3 pb-3">
          <RuleEditor
            rule={rule}
            outbounds={outbounds}
            onChange={onChange}
          />
        </div>
      )}
    </div>
  );
}
