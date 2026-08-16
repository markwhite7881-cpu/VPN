import { useCallback, useEffect, useRef, useState } from "react";
import { ArrowLeft, RefreshCw, Terminal } from "lucide-react";
import { vpnReadLogs } from "@/lib/vpn";
import { EmptyState } from "../components/EmptyState";
import { cn } from "@/lib/utils";

const inTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/**
 * Core log tail from the vpn plugin. Auto-scrolls to the bottom
 * while the user is already near the bottom (so reading history
 * isn't yanked away on refresh).
 */
export function LogsScreen({ onBack }: { onBack: () => void }) {
  const [text, setText] = useState<string>("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement | null>(null);

  const refresh = useCallback(async () => {
    if (!inTauri) {
      setText("preview mode — logs are available on Android builds");
      return;
    }
    setLoading(true);
    try {
      const t = await vpnReadLogs(300);
      setText(t);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // Stick to bottom on new content when already near the end.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const nearBottom =
      el.scrollHeight - el.scrollTop - el.clientHeight < 120;
    if (nearBottom) el.scrollTop = el.scrollHeight;
  }, [text]);

  // Defensive: a malformed plugin payload must never take down the
  // whole app (this screen is mounted eagerly by MobileApp).
  const lines = text ? String(text).split("\n") : [];

  return (
    <div className="flex h-full flex-col gap-3 p-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={onBack}
            aria-label="Back to settings"
            className="flex h-8 w-8 items-center justify-center rounded-md border border-border text-foreground hover:bg-accent"
          >
            <ArrowLeft className="h-4 w-4" />
          </button>
          <h2 className="text-base font-semibold tracking-tight">Logs</h2>
        </div>
        <button
          type="button"
          onClick={() => void refresh()}
          disabled={loading}
          className="flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-xs text-foreground hover:bg-accent disabled:opacity-50"
        >
          <RefreshCw className={cn("h-3.5 w-3.5", loading && "animate-spin")} />
          Refresh
        </button>
      </div>

      {error && (
        <p className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
          {error}
        </p>
      )}

      {lines.length === 0 ? (
        <EmptyState
          icon={Terminal}
          title="No log lines"
          hint="The core writes here once the VPN has started."
        />
      ) : (
        <div
          ref={scrollRef}
          className="min-h-0 flex-1 overflow-y-auto rounded-lg border border-border bg-card p-3"
        >
          <pre className="whitespace-pre-wrap break-all font-mono text-[10px] leading-relaxed text-foreground/80">
            {lines.map((line, i) => (
              <div
                key={i}
                className={cn(
                  /error|fatal|panic/i.test(line) && "text-destructive",
                  /warn/i.test(line) && "text-amber-300",
                )}
              >
                {line || " "}
              </div>
            ))}
          </pre>
        </div>
      )}
    </div>
  );
}
