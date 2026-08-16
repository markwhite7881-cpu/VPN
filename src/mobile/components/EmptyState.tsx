import type { LucideIcon } from "lucide-react";

/** Centered placeholder for empty lists. */
export function EmptyState({
  icon: Icon,
  title,
  hint,
}: {
  icon: LucideIcon;
  title: string;
  hint?: string;
}) {
  return (
    <div className="flex flex-col items-center gap-2 rounded-lg border border-dashed border-border px-4 py-10 text-center">
      <Icon className="h-6 w-6 text-muted-foreground/60" />
      <p className="text-sm font-medium text-muted-foreground">{title}</p>
      {hint && <p className="max-w-[260px] text-xs text-muted-foreground/70">{hint}</p>}
    </div>
  );
}
