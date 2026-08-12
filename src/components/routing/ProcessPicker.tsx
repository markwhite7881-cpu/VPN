// ProcessPicker — small inline panel that lists currently-running
// processes so the user can pick a .exe to add to a routing rule's
// `process_name` matcher.
//
// UI shape:
//   - Toggle button: "Pick from running…"  /  "Hide"
//   - When open: a search box + a scrollable list of processes.
//   - Click a row → toggle its name in the rule's `process_name` array.
//     A small badge on the right shows whether it's already selected.
//
// Behaviour:
//   - Fetches via `api.listProcesses()` on first open; refreshes
//     when the user clicks the refresh icon.
//   - Outside the Tauri shell (vite dev preview) the list is empty
//     and a hint is shown — we don't try to read the browser's
//     process list (it can't, and shouldn't).
//   - Already-selected processes are de-duplicated against the
//     `selected` array (case-insensitive). The list still shows them
//     so the user can see what's already in the rule.

import { useEffect, useMemo, useState } from "react";
import { ListChecks, Loader2, RefreshCw, X } from "lucide-react";
import { Button } from "../Button";
import { api, TauriCommandError } from "@/lib/api";
import type { ProcessInfo } from "@/lib/types";
import { cn } from "@/lib/utils";

interface Props {
  /** Currently selected process names (the rule's `process_name` array). */
  selected: string[];
  /** Called with the next value of the array. */
  onChange: (next: string[]) => void;
}

const SELECTED_KEY = (n: string) => n.toLowerCase();

export function ProcessPicker({ selected, onChange }: Props) {
  const [open, setOpen] = useState(false);
  const [processes, setProcesses] = useState<ProcessInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  const selectedSet = useMemo(
    () => new Set(selected.map(SELECTED_KEY)),
    [selected],
  );

  const fetchList = async () => {
    setLoading(true);
    setError(null);
    try {
      const procs = await api.listProcesses();
      setProcesses(procs);
    } catch (e) {
      const msg =
        e instanceof TauriCommandError
          ? `${e.kind}: ${e.message}`
          : e instanceof Error
            ? e.message
            : String(e);
      setError(msg);
    } finally {
      setLoading(false);
    }
  };

  // Lazy-load on first open.
  useEffect(() => {
    if (open && processes.length === 0 && !loading && !error) {
      void fetchList();
    }
    // We intentionally don't add `processes` / `loading` / `error` deps:
    // the open-once pattern is exactly what we want here.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return processes;
    return processes.filter((p) => p.name.toLowerCase().includes(q));
  }, [processes, query]);

  const toggle = (name: string) => {
    const key = SELECTED_KEY(name);
    if (selectedSet.has(key)) {
      onChange(selected.filter((n) => SELECTED_KEY(n) !== key));
    } else {
      onChange([...selected, name]);
    }
  };

  return (
    <div className="mt-2 rounded-md border border-border bg-background/40">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="w-full flex items-center justify-between px-2.5 py-1.5 text-xs text-muted-foreground hover:text-foreground"
      >
        <span className="flex items-center gap-1.5">
          <ListChecks size={12} />
          {open ? "Hide process list" : "Pick from running processes…"}
        </span>
        {selected.length > 0 && (
          <span className="text-[10px] text-foreground/80">
            {selected.length} selected
          </span>
        )}
      </button>
      {open && (
        <div className="border-t border-border p-2 space-y-2">
          {/* Search + refresh */}
          <div className="flex items-center gap-1.5">
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search process name…"
              className="flex-1 rounded-md bg-background border border-input px-2 py-1 text-xs text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
            />
            <Button
              variant="ghost"
              size="sm"
              onClick={fetchList}
              disabled={loading}
              title="Refresh"
            >
              {loading ? (
                <Loader2 size={12} className="animate-spin" />
              ) : (
                <RefreshCw size={12} />
              )}
            </Button>
          </div>

          {/* Status line */}
          {error ? (
            <div className="rounded border border-destructive/40 bg-destructive/10 px-2 py-1 text-[11px] text-destructive-foreground flex items-center justify-between">
              <span>Failed to list processes: {error}</span>
              <button
                type="button"
                onClick={() => setError(null)}
                className="text-destructive-foreground/80 hover:text-destructive-foreground"
                aria-label="Dismiss error"
              >
                <X size={12} />
              </button>
            </div>
          ) : loading && processes.length === 0 ? (
            <div className="text-[11px] text-muted-foreground px-1">
              Loading processes…
            </div>
          ) : processes.length === 0 ? (
            <div className="text-[11px] text-muted-foreground px-1">
              No processes available. (If you opened this in a browser
              preview, run the Tauri shell to enumerate real processes.)
            </div>
          ) : (
            <div className="text-[11px] text-muted-foreground px-1">
              {filtered.length} of {processes.length}
              {query && ` matching “${query}”`}
            </div>
          )}

          {/* List */}
          {filtered.length > 0 && (
            <div className="max-h-56 overflow-y-auto rounded border border-border bg-background">
              {filtered.map((p) => {
                const active = selectedSet.has(SELECTED_KEY(p.name));
                return (
                  <button
                    key={p.pid}
                    type="button"
                    onClick={() => toggle(p.name)}
                    className={cn(
                      "w-full flex items-center justify-between gap-2 px-2 py-1 text-xs text-left transition",
                      "hover:bg-accent",
                      active && "bg-primary/15",
                    )}
                  >
                    <span className="flex items-center gap-2 min-w-0">
                      <span
                        className={cn(
                          "inline-block w-3 h-3 rounded-sm border flex-shrink-0",
                          active
                            ? "bg-primary border-primary"
                            : "border-input",
                        )}
                        aria-hidden
                      />
                      <span className="truncate text-foreground">
                        {p.name}
                      </span>
                    </span>
                    <span className="text-[10px] text-muted-foreground font-mono">
                      pid {p.pid}
                    </span>
                  </button>
                );
              })}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
