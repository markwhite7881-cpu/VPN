// RuleList — sortable list of CustomRule with @dnd-kit.
//
// Owns the "which row is expanded" state (single-expand, like a
// normal details/summary tree). Emits rule changes / reorders / deletes
// up to the parent. The parent owns the actual rule array and persists
// it via the lifted `GeneratorSettings` callback.

import { useState } from "react";
import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  arrayMove,
  sortableKeyboardCoordinates,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { Plus } from "lucide-react";
import { Button } from "../Button";
import { RuleRow } from "./RuleRow";
import type { CustomRule, Outbound } from "@/lib/types";

interface Props {
  rules: CustomRule[];
  outbounds: Outbound[];
  /** Called when the list reorders, edits or deletes a rule. */
  onChange: (next: CustomRule[]) => void;
  /** Called when user clicks "Add rule" — parent creates a fresh rule. */
  onAdd: () => void;
}

export function RuleList({ rules, outbounds, onChange, onAdd }: Props) {
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  const handleDragEnd = (e: DragEndEvent) => {
    const { active, over } = e;
    if (!over || active.id === over.id) return;
    const oldIdx = rules.findIndex((r) => r.id === active.id);
    const newIdx = rules.findIndex((r) => r.id === over.id);
    if (oldIdx < 0 || newIdx < 0) return;
    onChange(arrayMove(rules, oldIdx, newIdx));
  };

  const updateRule = (next: CustomRule) => {
    onChange(rules.map((r) => (r.id === next.id ? next : r)));
  };
  const deleteRule = (id: string) => {
    if (expandedId === id) setExpandedId(null);
    onChange(rules.filter((r) => r.id !== id));
  };

  return (
    <div className="space-y-2">
      <DndContext
        sensors={sensors}
        collisionDetection={closestCenter}
        onDragEnd={handleDragEnd}
      >
        <SortableContext
          items={rules.map((r) => r.id)}
          strategy={verticalListSortingStrategy}
        >
          <div className="space-y-1.5">
            {rules.length === 0 && (
              <div className="rounded-md border border-dashed border-border bg-card/20 p-6 text-center text-sm text-muted-foreground">
                No rules yet. Click <strong className="text-foreground/80">+ Add rule</strong> below
                or pick a preset from the panel.
              </div>
            )}
            {rules.map((rule) => (
              <RuleRow
                key={rule.id}
                rule={rule}
                outbounds={outbounds}
                expanded={expandedId === rule.id}
                onToggleExpanded={() =>
                  setExpandedId((cur) => (cur === rule.id ? null : rule.id))
                }
                onChange={updateRule}
                onDelete={() => deleteRule(rule.id)}
              />
            ))}
          </div>
        </SortableContext>
      </DndContext>

      <Button variant="outline" onClick={onAdd} className="w-full">
        <Plus size={14} className="mr-1.5" />
        Add rule
      </Button>
    </div>
  );
}
