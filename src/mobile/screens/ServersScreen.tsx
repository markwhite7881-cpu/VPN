import { useMemo, useState } from "react";
import { Gauge, Link2, Loader2, Plus, RefreshCw, Server, Trash2 } from "lucide-react";
import { FlagIcon } from "@/components/FlagIcon";
import { api } from "@/lib/api";
import { flagForProfile } from "@/lib/flags";
import { isSupported, profileEndpoint, profileLabel } from "@/lib/outbound";
import { useServerLatency } from "@/hooks/useServerLatency";
import { cn } from "@/lib/utils";
import type { Outbound, Subscription } from "@/lib/types";
import { AddSubscriptionSheet } from "../components/AddSubscriptionSheet";
import { EmptyState } from "../components/EmptyState";
import { SectionCard, SectionHeader } from "../components/SectionCard";
import { formatMs } from "../lib/format";
import { latencyTone } from "../lib/mobileUi";

const PROBE_TIMEOUT_MS = 2_000;

export function ServersScreen({
  profiles,
  selectedIndex,
  geoipByIp,
  onSelect,
  subs,
  subFetching,
  onAddSub,
  onAddLinks,
  onRemoveSub,
  onRefreshSub,
}: {
  profiles: Outbound[];
  selectedIndex: number;
  geoipByIp: Record<string, string>;
  onSelect: (index: number) => void;
  subs: Subscription[];
  subFetching: Record<string, boolean>;
  onAddSub: (input: { name?: string; url: string }) => void;
  onAddLinks: (outbounds: Outbound[]) => void;
  onRemoveSub: (id: string) => void;
  onRefreshSub: (id: string) => void;
}) {
  // Auto-probe every 10 s (reused desktop hook); "Ping all" fires an
  // extra manual pass and overlays its results on the hook's map.
  const auto = useServerLatency(profiles);
  const [manual, setManual] = useState<Map<string, number>>(new Map());
  const [pinging, setPinging] = useState(false);
  const [sheetOpen, setSheetOpen] = useState(false);

  const latencyByTag = useMemo(() => {
    const merged = new Map(auto.byTag);
    for (const [k, v] of manual) merged.set(k, v);
    return merged;
  }, [auto.byTag, manual]);

  const pingAll = async () => {
    const supported = profiles.filter(isSupported);
    if (supported.length === 0 || pinging) return;
    setPinging(true);
    try {
      const results = await Promise.allSettled(
        supported.map((p) =>
          api
            .pingEndpoint(p.server, p.port, PROBE_TIMEOUT_MS)
            .then((d) => ({ tag: p.tag, ms: d }))
            .catch(() => ({ tag: p.tag, ms: null as number | null })),
        ),
      );
      const next = new Map<string, number>();
      for (const r of results) {
        if (r.status === "fulfilled" && r.value.ms != null) {
          next.set(r.value.tag, r.value.ms);
        }
      }
      setManual(next);
    } finally {
      setPinging(false);
    }
  };

  return (
    <div className="flex flex-col gap-4 p-4">
      <div className="flex items-center justify-between">
        <h2 className="text-base font-semibold tracking-tight">Servers</h2>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => void pingAll()}
            disabled={pinging || profiles.length === 0}
            className="flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-xs text-foreground hover:bg-accent disabled:opacity-50"
          >
            {pinging ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Gauge className="h-3.5 w-3.5" />
            )}
            Ping all
          </button>
          <button
            type="button"
            onClick={() => setSheetOpen(true)}
            aria-label="Add subscription"
            className="flex h-8 w-8 items-center justify-center rounded-md bg-primary text-primary-foreground shadow-sm hover:bg-primary/90"
          >
            <Plus className="h-4 w-4" />
          </button>
        </div>
      </div>

      {profiles.length === 0 ? (
        <EmptyState
          icon={Server}
          title="No servers yet"
          hint="Use + to paste either a subscription URL or a direct share link; your servers will appear here."
        />
      ) : (
        <SectionCard>
          <ul className="divide-y divide-border">
            {profiles.map((o, i) => {
              const supported = isSupported(o);
              const isSel = i === selectedIndex;
              const ms =
                supported && latencyByTag.has(o.tag)
                  ? latencyByTag.get(o.tag)
                  : undefined;
              const { code } = flagForProfile({
                tag: supported ? o.tag : undefined,
                server: supported ? o.server : undefined,
                geoipByIp,
              });
              return (
                <li key={`srv-${i}-${supported ? profileEndpoint(o) : "x"}`}>
                  <button
                    type="button"
                    onClick={() => supported && onSelect(i)}
                    disabled={!supported}
                    className={cn(
                      "flex w-full items-center gap-3 px-3.5 py-3 text-left transition-colors",
                      isSel ? "bg-foreground/5" : "hover:bg-accent/60",
                      !supported && "opacity-50",
                    )}
                  >
                    {/* Radio-style selected indicator. */}
                    <span
                      className={cn(
                        "flex h-4 w-4 shrink-0 items-center justify-center rounded-full border",
                        isSel ? "border-emerald-400/70" : "border-muted-foreground/40",
                      )}
                      aria-hidden
                    >
                      {isSel && (
                        <span className="h-2 w-2 rounded-full bg-emerald-400" />
                      )}
                    </span>
                    <FlagIcon code={code} size={18} className="shrink-0" />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-sm font-medium">
                        {profileLabel(o)}
                      </span>
                      <span className="block truncate font-mono text-[11px] text-muted-foreground">
                        {supported ? profileEndpoint(o) : "unsupported link"}
                      </span>
                    </span>
                    <LatencyBadge ms={ms} dim={pinging && ms == null} />
                  </button>
                </li>
              );
            })}
          </ul>
        </SectionCard>
      )}

      {/* Subscriptions overview. */}
      {subs.length > 0 && (
        <SectionCard>
          <SectionHeader title="Subscriptions" />
          <ul className="divide-y divide-border">
            {subs.map((s) => (
              <li key={s.id} className="flex items-center gap-2 px-3.5 py-2.5">
                <Link2 className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm">{s.name}</p>
                  <p className="truncate text-[11px] text-muted-foreground">
                    {s.lastError ? (
                      <span className="text-destructive">{s.lastError}</span>
                    ) : (
                      `${s.lastCount} servers`
                    )}
                  </p>
                </div>
                <button
                  type="button"
                  onClick={() => void onRefreshSub(s.id)}
                  disabled={subFetching[s.id]}
                  aria-label={`Refresh ${s.name}`}
                  className="flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-50"
                >
                  <RefreshCw
                    className={cn(
                      "h-3.5 w-3.5",
                      subFetching[s.id] && "animate-spin",
                    )}
                  />
                </button>
                <button
                  type="button"
                  onClick={() => onRemoveSub(s.id)}
                  aria-label={`Remove ${s.name}`}
                  className="flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </button>
              </li>
            ))}
          </ul>
        </SectionCard>
      )}

      <AddSubscriptionSheet
        open={sheetOpen}
        onClose={() => setSheetOpen(false)}
        onAdd={onAddSub}
        onAddLinks={onAddLinks}
      />
    </div>
  );
}

/** Latency badge uses the shared fast/medium/slow policy. */
function LatencyBadge({ ms, dim }: { ms: number | undefined; dim?: boolean }) {
  const tone = latencyTone(ms);
  if (tone === "pending") {
    return (
      <span
        aria-label={dim ? "Latency unavailable after ping" : "Latency not measured"}
        className={cn(
          "rounded-full border border-border px-2 py-0.5 font-mono text-[10px] tabular-nums",
          dim ? "text-muted-foreground/40" : "text-muted-foreground/70",
        )}
      >
        —
      </span>
    );
  }
  const toneClass =
    tone === "fast"
      ? "border-emerald-400/30 bg-emerald-500/10 text-emerald-300"
      : tone === "medium"
        ? "border-amber-400/30 bg-amber-500/10 text-amber-300"
        : "border-red-400/30 bg-red-500/10 text-red-300";
  return (
    <span
      aria-label={`Latency ${ms} milliseconds, ${tone}`}
      className={cn(
        "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 font-mono text-[10px] tabular-nums",
        toneClass,
      )}
    >
      <span aria-hidden className="h-1.5 w-1.5 rounded-full bg-current" />
      {formatMs(ms)}
    </span>
  );
}
