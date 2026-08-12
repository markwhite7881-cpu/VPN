import { forwardRef, type HTMLAttributes } from "react";
import { cn } from "@/lib/utils";

export type BadgeVariant =
  | "default"
  | "secondary"
  | "destructive"
  | "outline";

const VARIANT: Record<BadgeVariant, string> = {
  default: "bg-foreground/10 text-foreground border-foreground/15",
  secondary: "bg-muted text-secondary-foreground border-border",
  destructive: "bg-destructive/10 text-destructive border-destructive/30",
  outline: "border-border text-foreground bg-transparent",
};

interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  variant?: BadgeVariant;
}

export const Badge = forwardRef<HTMLSpanElement, BadgeProps>(
  ({ className, variant = "default", ...props }, ref) => {
    return (
      <span
        ref={ref}
        className={cn(
          "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-xs font-medium",
          VARIANT[variant],
          className
        )}
        {...props}
      />
    );
  }
);
Badge.displayName = "Badge";
