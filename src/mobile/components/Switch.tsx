import { cn } from "@/lib/utils";

/**
 * Toggle switch styled as a checkbox replacement. Emerald accent
 * when on — the single accent colour used across the mobile UI for
 * "active" states (matches the connected glow on the Home screen).
 */
export function Switch({
  checked,
  onChange,
  disabled,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
  /** Accessible name when the row has no visible label. */
  label?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative h-6 w-11 shrink-0 rounded-full border transition-colors",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        "disabled:cursor-not-allowed disabled:opacity-50",
        checked
          ? "border-emerald-400/50 bg-emerald-500/25"
          : "border-border bg-muted",
      )}
    >
      <span
        className={cn(
          "absolute top-1/2 h-4 w-4 -translate-y-1/2 rounded-full transition-all",
          checked
            ? "left-[calc(100%-1.25rem)] bg-emerald-300"
            : "left-1 bg-muted-foreground",
        )}
      />
    </button>
  );
}
