import { useEffect, useRef, useState } from "react";
import {
  Activity,
  ChevronDown,
  Loader2,
  Power,
  Server,
  Sparkles,
  TrendingDown,
  TrendingUp,
} from "lucide-react";
import { Card, CardContent } from "@/components/Card";
import { Badge } from "@/components/Badge";
import { FlagIcon } from "@/components/FlagIcon";
import { UpdateCard } from "@/components/UpdateCard";
import { useTrafficStream } from "@/hooks/useTrafficStream";
import { latencyToBars, useServerLatency } from "@/hooks/useServerLatency";
import { cn } from "@/lib/utils";
import { flagForProfile } from "@/lib/flags";
import { profileLabel, profileEndpoint, isSupported } from "@/lib/outbound";
import { isValidProfileSelection } from "@/lib/profileSelection";
import type { Outbound, Status, StatusReport } from "@/lib/types";

const inTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export interface HomeTabProps {
  status: StatusReport;
  statusLabel: Status;
  busy: boolean;
  error: string | null;
  canStart: boolean;
  configName: string | null;
  profiles: Outbound[];
  selectedIndex: number;
  /**
   * Tag of the outbound the running `proxy` selector is currently
   * routing through, as reported by the clash API. `null` while
   * sing-box is stopped or while we haven't polled yet.
   * "auto" means the `auto` urltest is in control.
   */
  activeOutbound: string | null;
  /** ip → country-code map, populated by the useGeoIp hook. */
  geoipByIp: Record<string, string>;
  /** sing-box version string, fetched on app start. */
  currentSingboxVersion: string | null;
  /** Fires after a successful sing-box auto-update so the parent
   *  can re-fetch the version. */
  onSingboxUpdated: () => void;
  onSelect: (index: number) => void;
  onConnect: () => void;
  onDisconnect: () => void;
}

export function HomeTab({
  status,
  statusLabel,
  busy,
  error,
  canStart,
  configName,
  profiles,
  selectedIndex,
  activeOutbound,
  geoipByIp,
  currentSingboxVersion,
  onSingboxUpdated,
  onSelect,
  onConnect,
  onDisconnect,
}: HomeTabProps) {
  const isRunning = statusLabel === "running";
  const isTransition = statusLabel === "starting" || statusLabel === "stopping";
  const trafficLive = useTrafficStream(isRunning || !inTauri, profiles.length);
  const current = trafficLive.current;
  // Per-server latency probe (clash API `/proxies/{tag}/delay`,
  // refreshed every 10 s while the tunnel is up). Shown next to
  // each server in the picker and the all-servers grid.
  const latency = useServerLatency(profiles, isRunning);

  const headline = (() => {
    if (statusLabel === "running") return "Connected";
    if (statusLabel === "starting") return "Connecting…";
    if (statusLabel === "stopping") return "Disconnecting…";
    if (statusLabel === "crashed") return "Crashed";
    return "Disconnected";
  })();

  // Clamp the selected index whenever the list shrinks so we never
  // point at a missing server.
  useEffect(() => {
    if (profiles.length === 0 && selectedIndex !== -1) onSelect(-1);
    else if (profiles.length > 0 && !isValidProfileSelection(selectedIndex, profiles.length))
      onSelect(0);
  }, [profiles.length, selectedIndex, onSelect]);

  const selected = profiles[selectedIndex];

  // Resolve the live `activeOutbound` tag (if any) to a profile, so
  // we can show the flag + friendly name in the hero. "auto" is
  // special — the urltest group is in charge, not a single server.
  const activeIsAuto = activeOutbound === "auto";
  const activeProfile = activeOutbound
    ? profiles.find((p) => isSupported(p) && p.tag === activeOutbound)
    : undefined;
  // `find` doesn't narrow the type, so we apply `isSupported`
  // again before reading `tag` / `server` — both fields are
  // missing on the `Outbound.Unsupported` variant.
  const activeSupported =
    activeProfile && isSupported(activeProfile) ? activeProfile : null;
  const activeFlag = activeIsAuto
    ? { flag: "🌐", code: "??" }
    : activeSupported
      ? flagForProfile({
          tag: activeSupported.tag,
          server: activeSupported.server,
          geoipByIp,
        })
      : null;
  const activeName = activeIsAuto
    ? "Auto (urltest)"
    : activeSupported
      ? profileLabel(activeSupported)
      : null;
  // Mismatch = user pinned one server, but the running selector is
  // on a different one. Happens with "Auto" once urltest migrates.
  // We surface this so the user can see at a glance why a request
  // that picked "Казахстан" went through "Нидерланды" instead.
  const activeMatchesPicked =
    activeOutbound == null ||
    (selectedIndex >= 0 && selected && isSupported(selected) && selected.tag === activeOutbound);
  const userPicked = !activeMatchesPicked
    ? selectedIndex === -1
      ? "Auto"
      : selected && isSupported(selected)
        ? profileLabel(selected)
        : null
    : null;

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-4 p-6">
      {/* Hero — the single most prominent surface in the app. */}
      <Card className="overflow-hidden">
        <CardContent className="flex flex-col items-center gap-6 p-10 text-center">
          {/* Server picker (above the icon) */}
          <ServerPicker
            profiles={profiles}
            selectedIndex={selectedIndex}
            latencyByTag={latency.byTag}
            geoipByIp={geoipByIp}
            onSelect={onSelect}
          />

          {/* The power button is BOTH the icon and the action — no
              separate "Connect" / "Disconnect" pill below. One click
              toggles the tunnel; its visual state (border + glow +
              icon) tells the user whether they're connected. The
              green-when-connected treatment borrows the universal
              "ready" colour so the state is readable at a glance,
              even before you read the headline. */}
          <button
            type="button"
            onClick={isRunning ? onDisconnect : onConnect}
            disabled={busy || isTransition || (!isRunning && !canStart)}
            aria-label={isRunning ? "Disconnect" : "Connect"}
            className={cn(
              "group relative flex h-20 w-20 items-center justify-center rounded-full",
              "border-2 transition-all duration-200",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-foreground/40",
              "disabled:cursor-not-allowed",
              "shadow-lg",
              isRunning
                ? "border-emerald-400/60 bg-emerald-500/15 text-emerald-300 hover:border-emerald-400/80 hover:bg-emerald-500/25 hover:shadow-emerald-500/20"
                : isTransition
                  ? "border-foreground/20 bg-foreground/5"
                  : "border-muted-foreground/40 bg-muted/40 text-muted-foreground hover:border-foreground/50 hover:bg-muted/60 hover:text-foreground",
            )}
          >
            {isTransition ? (
              <Loader2 className="h-8 w-8 animate-spin text-foreground/70" />
            ) : (
              <Power
                className={cn(
                  "h-8 w-8 transition-transform group-hover:scale-110",
                  isRunning
                    ? "text-emerald-300 drop-shadow-[0_0_6px_rgba(52,211,153,0.5)]"
                    : "",
                )}
              />
            )}
          </button>
          <div className="space-y-1">
            <div className="flex items-center justify-center gap-2">
              <h1 className="text-2xl font-semibold tracking-tight">{headline}</h1>
              {isRunning && (
                <Badge variant="default" className="text-[10px]">
                  <Sparkles className="h-3 w-3" />
                  live
                </Badge>
              )}
            </div>
            <p className="flex items-center justify-center gap-1.5 text-sm text-muted-foreground">
              {isRunning
                ? activeName && activeFlag
                  ? <>
                      <span>via</span>
                      <FlagIcon code={activeFlag.code} size={14} className="self-center" />
                      <span>{activeName}</span>
                    </>
                  : "sing-box is running."
                : profiles.length === 0
                  ? "Add a server in the Servers tab to get started."
                  : "Click the power button to bring the tunnel up."}
            </p>
            {isRunning && userPicked && (
              <p className="font-mono text-[10px] text-muted-foreground/70">
                picked: {userPicked}
              </p>
            )}
          </div>

          {!canStart && !isRunning && !isTransition && (
            <p className="text-[11px] text-muted-foreground">
              {profiles.length === 0
                ? "No servers yet — head to Servers to import a link."
                : "Click above to bring the tunnel up."}
            </p>
          )}
        </CardContent>
      </Card>

      {/* Live traffic — Download is the "incoming" channel and
          gets the cool emerald accent; Upload is the "outgoing"
          channel and gets the warm amber accent. The two colours
          sit on opposite sides of the wheel so the difference
          reads at a glance even for users with mild colour
          vision deficiency, and the icons (↘/↗) keep the
          direction readable if the colour doesn't carry. */}
      <div className="grid grid-cols-2 gap-3">
        <Card>
          <CardContent className="p-4">
            <div className="flex items-center gap-1.5 text-[10px] uppercase tracking-wider text-emerald-400/90">
              <TrendingDown className="h-3 w-3" />
              Download
            </div>
            <p className="mt-1.5 font-mono text-2xl font-semibold tabular-nums text-emerald-300">
              {formatRate(current?.down_bps ?? 0)}
            </p>
            <p className="mt-0.5 font-mono text-[10px] text-emerald-400/60">
              total {formatBytes(current?.down_total ?? 0)}
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-4">
            <div className="flex items-center gap-1.5 text-[10px] uppercase tracking-wider text-amber-400/90">
              <TrendingUp className="h-3 w-3" />
              Upload
            </div>
            <p className="mt-1.5 font-mono text-2xl font-semibold tabular-nums text-amber-300">
              {formatRate(current?.up_bps ?? 0)}
            </p>
            <p className="mt-0.5 font-mono text-[10px] text-amber-400/60">
              total {formatBytes(current?.up_total ?? 0)}
            </p>
          </CardContent>
        </Card>
      </div>

      {/* Process info */}
      <Card>
        <CardContent className="grid grid-cols-3 gap-4 p-4 text-center sm:grid-cols-3">
          <Metric label="Status" value={statusLabel} />
          <Metric
            label="Uptime"
            value={formatUptime(status.uptime_secs)}
            mono
          />
          <Metric
            label="Config"
            value={configName || "—"}
            mono
          />
        </CardContent>
      </Card>

      {error && (
        <Card className="border-destructive/30 bg-destructive/5">
          <CardContent className="flex items-start gap-2 p-3 text-xs text-destructive">
            <Activity className="mt-0.5 h-3.5 w-3.5 shrink-0" />
            <span className="break-words">{error}</span>
          </CardContent>
        </Card>
      )}

      {/* All-servers strip — quick visual switcher. */}
      {profiles.length > 0 && (
        <Card>
          <CardContent className="p-3">
            <div className="mb-2 flex items-center gap-1.5 px-1 text-[10px] uppercase tracking-wider text-muted-foreground">
              <Server className="h-3 w-3" />
              Servers ({profiles.length})
            </div>
            <div className="grid grid-cols-2 gap-1.5 sm:grid-cols-3">
              {profiles.map((o, i) => {
                const isSel = i === selectedIndex;
                const supported = isSupported(o);
                const { code } = flagForProfile({
                  tag: supported ? o.tag : undefined,
                  server: supported ? o.server : undefined,
                  geoipByIp,
                });
                const ms = supported ? latency.byTag.get(o.tag) : undefined;
                return (
                  <button
                    key={`picker-${i}-${profileEndpoint(o) || "x"}`}
                    onClick={() => onSelect(i)}
                    className={cn(
                      "flex items-center gap-1.5 rounded-md border px-2 py-1.5 text-left text-xs transition-colors",
                      isSel
                        ? "border-foreground/30 bg-foreground/5"
                        : "border-border bg-card/30 hover:bg-accent",
                    )}
                  >
                    <FlagIcon code={code} size={14} className="shrink-0 self-center" />
                    <span className="min-w-0 flex-1 truncate font-medium">
                      {profileLabel(o)}
                    </span>
                    <LatencyBadge ms={ms} />
                  </button>
                );
              })}
            </div>
          </CardContent>
        </Card>
      )}

      {/* App shell + sing-box auto-update. Auto-checks on mount; the
          user can force a refresh with the per-row Check button. */}
      <UpdateCard
        currentSingboxVersion={currentSingboxVersion}
        onSingboxUpdated={onSingboxUpdated}
      />
    </div>
  );
}

/**
 * Compact dropdown-style server picker rendered above the hero icon.
 * Click → menu with the full list; click outside → closes.
 */
function ServerPicker({
  profiles,
  selectedIndex,
  latencyByTag,
  geoipByIp,
  onSelect,
}: {
  profiles: Outbound[];
  selectedIndex: number;
  latencyByTag: Map<string, number>;
  geoipByIp: Record<string, string>;
  onSelect: (i: number) => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const onEsc = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onEsc);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onEsc);
    };
  }, [open]);

  const selected = profiles[selectedIndex];
  const selectedFlag = selected
    ? flagForProfile({
        tag: isSupported(selected) ? selected.tag : undefined,
        server: isSupported(selected) ? selected.server : undefined,
        geoipByIp,
      })
    : { flag: "🌐", code: "??" };
  const selectedName = selected ? profileLabel(selected) : "No servers";
  const selectedMs = selected && isSupported(selected) ? latencyByTag.get(selected.tag) : undefined;

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        disabled={profiles.length === 0}
        className={cn(
          "flex items-center gap-2 rounded-full border border-border bg-card/60 px-3 py-1.5 text-xs",
          "hover:bg-accent disabled:opacity-50",
        )}
      >
        <FlagIcon code={selectedFlag.code} size={16} className="shrink-0" />
        <span className="max-w-[180px] truncate font-medium">{selectedName}</span>
        <LatencyBadge ms={selectedMs} compact />
        <ChevronDown
          className={cn(
            "h-3 w-3 text-muted-foreground transition-transform",
            open && "rotate-180",
          )}
        />
      </button>
      {open && profiles.length > 0 && (
        <div
          className={cn(
            "absolute left-1/2 top-full z-20 mt-2 -translate-x-1/2",
            "w-80 max-h-96 overflow-auto rounded-md border border-border bg-card shadow-2xl",
          )}
        >
          <ul className="p-1">
            {/* "Auto" entry — resets default_outbound to null so
                the `auto` urltest decides based on latency. We use
                index -1 to mean "no specific pick". */}
            <li>
              <button
                type="button"
                onClick={() => {
                  onSelect(-1);
                  setOpen(false);
                }}
                className={cn(
                  "flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs",
                  selectedIndex === -1
                    ? "bg-foreground/5"
                    : "hover:bg-accent",
                )}
              >
                <span
                  className="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full border border-border bg-muted text-[8px] text-muted-foreground"
                  aria-hidden
                >
                  ∞
                </span>
                <span className="flex-1 truncate font-medium">
                  Auto (best latency)
                </span>
                <span className="font-mono text-[10px] text-muted-foreground">
                  urltest
                </span>
              </button>
            </li>
            {profiles.map((o, i) => {
              const isSel = i === selectedIndex;
              const { code } = flagForProfile({
                tag: isSupported(o) ? o.tag : undefined,
                server: isSupported(o) ? o.server : undefined,
                geoipByIp,
              });
              const ms = isSupported(o) ? latencyByTag.get(o.tag) : undefined;
              return (
                <li key={`pick-${i}-${profileEndpoint(o) || "x"}`}>
                  <button
                    type="button"
                    onClick={() => {
                      onSelect(i);
                      setOpen(false);
                    }}
                    className={cn(
                      "flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs",
                      isSel ? "bg-foreground/5" : "hover:bg-accent",
                    )}
                  >
                    <FlagIcon code={code} size={16} className="shrink-0" />
                    <span className="min-w-0 flex-1 truncate font-medium">
                      {profileLabel(o)}
                    </span>
                    <span className="font-mono text-[10px] text-muted-foreground">
                      {o.protocol}
                    </span>
                    <LatencyBadge ms={ms} />
                  </button>
                </li>
              );
            })}
          </ul>
        </div>
      )}
    </div>
  );
}

/** Compact "▮▮▮▮ 47ms" badge. `compact` drops the bars (just the
 *  number) for the small pill at the top of the hero. */
function LatencyBadge({ ms, compact = false }: { ms: number | undefined; compact?: boolean }) {
  if (ms == null) {
    return (
      <span className="font-mono text-[10px] text-muted-foreground/60">—</span>
    );
  }
  const bars = latencyToBars(ms);
  return (
    <span className="flex items-center gap-1 font-mono text-[10px] tabular-nums text-muted-foreground">
      {!compact && <SignalBars level={bars} />}
      <span>{ms < 1000 ? `${Math.round(ms)}ms` : `${(ms / 1000).toFixed(1)}s`}</span>
    </span>
  );
}

/** Four little vertical bars, lit up to `level` (0..4). */
function SignalBars({ level }: { level: number }) {
  const heights = [3, 5, 7, 9];
  return (
    <span className="flex items-end gap-[1.5px]" aria-label={`signal ${level} of 4`}>
      {heights.map((h, i) => (
        <span
          key={i}
          className={cn(
            "w-[2px] rounded-sm",
            i < level ? "bg-foreground/70" : "bg-foreground/15",
          )}
          style={{ height: h }}
        />
      ))}
    </span>
  );
}

function Metric({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="space-y-1">
      <p className="text-[10px] uppercase tracking-wider text-muted-foreground">
        {label}
      </p>
      <p
        className={cn(
          "truncate text-sm",
          mono && "font-mono",
          (!value || value === "—") && "text-muted-foreground",
        )}
        title={value}
      >
        {value}
      </p>
    </div>
  );
}

function formatRate(bps: number): string {
  if (!bps) return "0 B/s";
  const units = ["B/s", "KB/s", "MB/s", "GB/s"];
  let v = bps;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 100 || i === 0 ? 0 : 1)} ${units[i]}`;
}

function formatBytes(b: number): string {
  if (!b) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  let v = b;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 100 || i === 0 ? 0 : 1)} ${units[i]}`;
}

function formatUptime(secs: number | null | undefined): string {
  if (secs == null) return "—";
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}
