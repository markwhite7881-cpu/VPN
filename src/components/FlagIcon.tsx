// Circular country-flag SVG from the `circle-flags` package.
//
// Why a custom component instead of using emoji (🇷🇺, 🇩🇪, …) directly:
//   WebView2 on Windows doesn't ship the emoji font that turns
//   regional-indicator pairs into coloured flags, so 🇩🇪 renders
//   as the bare two-letter code "DE" — fine for the geek audience
//   but ugly in the UI. We bundle the real SVG instead, which is
//   the only thing that survives every platform's font situation.
//
// The full set of country SVGs is included in the bundle (≈200
// files × ~400 bytes = ~80 KB), which we happily trade for the
// ability to render any country without touching the network.

import { type ImgHTMLAttributes } from "react";
import { cn } from "@/lib/utils";

// Vite globs every flag SVG at build time and gives us a map from
// relative path → hashed URL. `eager: true` makes them all
// available synchronously; `query: '?url'` resolves them to asset
// URLs (not raw text).
const FLAG_URLS = import.meta.glob(
  "../../node_modules/circle-flags/flags/*.svg",
  { eager: true, query: "?url", import: "default" },
) as Record<string, string>;

export interface FlagIconProps
  extends Omit<ImgHTMLAttributes<HTMLImageElement>, "src" | "alt"> {
  /** Two-letter ISO 3166-1 alpha-2 code, e.g. "RU", "DE", "NL". Case-insensitive. */
  code: string;
  /** Pixel size for both width and height. Defaults to 16. */
  size?: number;
  /** Optional accessible label; defaults to the code itself. */
  alt?: string;
}

/**
 * Renders a circular country flag as a small `<img>`. Falls back
 * to a tiny globe glyph when the code isn't in the bundle.
 */
export function FlagIcon({
  code,
  size = 16,
  alt,
  className,
  ...rest
}: FlagIconProps) {
  const key = code?.toLowerCase();
  const src = key ? FLAG_URLS[`../../node_modules/circle-flags/flags/${key}.svg`] : undefined;
  if (!src) {
    // Unknown / missing country code — render a neutral circle so
    // the layout doesn't shift. The "🌐" glyph also doesn't
    // render in WebView2, so we use plain text "··".
    return (
      <span
        className={cn(
          "inline-flex items-center justify-center rounded-full",
          "border border-border bg-muted text-[8px] text-muted-foreground",
          className,
        )}
        style={{ width: size, height: size }}
        aria-label={alt ?? code}
        title={alt ?? code}
      >
        ··
      </span>
    );
  }
  return (
    <img
      src={src}
      width={size}
      height={size}
      alt={alt ?? code}
      className={cn("inline-block rounded-full", className)}
      {...rest}
    />
  );
}
