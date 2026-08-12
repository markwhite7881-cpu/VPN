import { type ReactNode } from "react";
import { cn } from "@/lib/utils";

export interface TabDef {
  /** Stable key (used for localStorage and React keys). */
  id: string;
  /** Short label shown in the tab bar. */
  label: string;
  /** Optional badge content (count, status dot, etc.). */
  badge?: ReactNode;
  /** Lucide icon component. */
  icon: React.ComponentType<{ className?: string }>;
  /** Tab body. */
  content: ReactNode;
}

/**
 * Minimalist horizontal tab bar with a thin underline on the active
 * tab — matches the classquiz colour palette (no rounded pills,
 * no coloured accents beyond a single muted line).
 */
export function Tabs({
  tabs,
  active,
  onChange,
}: {
  tabs: TabDef[];
  active: string;
  onChange: (id: string) => void;
}) {
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div
        role="tablist"
        className="flex shrink-0 items-stretch gap-1 border-b border-border bg-card/30 px-2"
      >
        {tabs.map((t) => {
          const Icon = t.icon;
          const isActive = t.id === active;
          return (
            <button
              key={t.id}
              role="tab"
              aria-selected={isActive}
              onClick={() => onChange(t.id)}
              className={cn(
                "relative flex items-center gap-1.5 px-3 py-2 text-xs font-medium",
                "transition-colors",
                isActive
                  ? "text-foreground"
                  : "text-muted-foreground hover:text-foreground/80",
              )}
            >
              <Icon className="h-3.5 w-3.5" />
              <span>{t.label}</span>
              {t.badge != null && (
                <span
                  className={cn(
                    "ml-0.5 rounded px-1 py-0 text-[10px] tabular-nums",
                    isActive
                      ? "bg-foreground/10 text-foreground"
                      : "bg-muted text-muted-foreground",
                  )}
                >
                  {t.badge}
                </span>
              )}
              {isActive && (
                <span className="absolute inset-x-2 -bottom-px h-px bg-foreground" />
              )}
            </button>
          );
        })}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {tabs.find((t) => t.id === active)?.content}
      </div>
    </div>
  );
}
