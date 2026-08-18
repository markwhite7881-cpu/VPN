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
import type { HomeProfileMetadata, Status, StatusReport } from "@/lib/types";
import type { ConnectionProfile } from "@/lib/connectionProfiles";

const inTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export interface HomeTabProps {
  status: StatusReport;
  statusLabel: Status;
  busy: boolean;
  error: string | null;
  canStart: boolean;
  configName: string | null;
  profiles: ConnectionProfile[];
  selectedIndex: number;
  /**
   * Tag of the outbound the running `proxy` selector is currently
   * routing through, as reported by the clash API. `null` while
   * sing-box is stopped or while we haven't polled yet.
   * "auto" means the `auto` urltest is in control.
   */
  activeOutbound: string | null;
  /** Safe country and latency metadata for ready Xray profiles, keyed by `${subscriptionId}:${key}`. */
  readyProfileMetadata: ReadonlyMap<string, HomeProfileMetadata>;
  /** Safe subscription summary names keyed by subscription ID. */
  subscriptionNames: ReadonlyMap<string, string>;
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
  readyProfileMetadata,
  subscriptionNames,
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
  // Subscription summaries do not disclose endpoints, so only manual
  // profiles participate in endpoint latency probing.
  const latency = useServerLatency(
    profiles
      .filter((profile): profile is Extract<ConnectionProfile, { kind: "manual" }> => profile.kind === "manual")
      .map((profile) => profile.outbound),
    isRunning,
  );

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
  const isXrayRunning = isRunning && status.engine === "xray";
  const activeXrayProfile = isXrayRunning
    ? profiles.find(
        (profile): profile is Extract<ConnectionProfile, { kind: "ready_config" }> =>
          profile.kind === "ready_config" &&
          profile.engine === "xray" &&
          profile.key === status.profile_key,
      )
    : undefined;
  const activeXrayDisplay = activeXrayProfile
    ? connectionProfileDisplay(activeXrayProfile, readyProfileMetadata, geoipByIp, latency.byTag)
    : { flag: "🌐", code: "??", label: status.profile_name ?? "Xray", ms: undefined };

  // Resolve the live `activeOutbound` tag (if any) to a profile, so
  // we can show the flag + friendly name in the hero. "auto" is
  // special — the urltest group is in charge, not a single server.
  const activeIsAuto = activeOutbound === "auto";
  const activeProfile = activeOutbound
    ? profiles.find(
        (profile) =>
          profile.kind === "manual" &&
          isSupported(profile.outbound) &&
          profile.outbound.tag === activeOutbound,
      )
    : undefined;
  const activeSupported =
    activeProfile?.kind === "manual" && isSupported(activeProfile.outbound)
      ? activeProfile.outbound
      : null;
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
  const selectedManual = selected?.kind === "manual" ? selected.outbound : null;
  const activeMatchesPicked =
    activeOutbound == null ||
    (selectedIndex >= 0 && selectedManual && isSupported(selectedManual) && selectedManual.tag === activeOutbound);
  const userPicked = !activeMatchesPicked
    ? selectedIndex === -1
      ? "Auto"
      : selected
        ? connectionProfileLabel(selected)
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
            readyProfileMetadata={readyProfileMetadata}
            subscriptionNames={subscriptionNames}
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
              powerButtonClasses(statusLabel),
            )}
          >
            {isTransition ? (
              <Loader2 className="h-8 w-8 animate-spin text-foreground/70" />
            ) : (
              <Power
                className={cn(
                  "h-8 w-8 transition-transform group-hover:scale-110",
                  isRunning ? "text-success" : "",
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
                ? isXrayRunning
                  ? <>
                      <span>via</span>
                      <FlagIcon code={activeXrayDisplay.code} size={14} className="self-center" />
                      <span>{status.profile_name ?? activeXrayDisplay.label}</span>
                    </>
                  : activeName && activeFlag
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
            {isRunning && !isXrayRunning && userPicked && (
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

      {/* Live traffic direction remains distinguishable through labels and icons. */}
      <div className="grid grid-cols-2 gap-3">
        <Card>
          <CardContent className="p-4">
            <div className="flex items-center gap-1.5 text-[10px] uppercase tracking-wider text-muted-foreground">
              <TrendingDown className="h-3 w-3" />
              Download
            </div>
            <p className="mt-1.5 font-mono text-2xl font-semibold tabular-nums text-foreground">
              {formatRate(current?.down_bps ?? 0)}
            </p>
            <p className="mt-0.5 font-mono text-[10px] text-muted-foreground">
              total {formatBytes(current?.down_total ?? 0)}
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-4">
            <div className="flex items-center gap-1.5 text-[10px] uppercase tracking-wider text-muted-foreground">
              <TrendingUp className="h-3 w-3" />
              Upload
            </div>
            <p className="mt-1.5 font-mono text-2xl font-semibold tabular-nums text-foreground">
              {formatRate(current?.up_bps ?? 0)}
            </p>
            <p className="mt-0.5 font-mono text-[10px] text-muted-foreground">
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
            <GroupedHomeProfileRows
            profiles={profiles}
            selectedIndex={selectedIndex}
            readyProfileMetadata={readyProfileMetadata}
            subscriptionNames={subscriptionNames}
            geoipByIp={geoipByIp}
            latencyByTag={latency.byTag}
            onSelect={onSelect}
            mode="grid"
          />
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

export function powerButtonClasses(statusLabel: Status): string {
  if (statusLabel === "running") {
    return "border-success/40 bg-success/10 text-success shadow-success/20 hover:border-success/60 hover:bg-success/15 focus-visible:ring-success/40";
  }
  if (statusLabel === "starting" || statusLabel === "stopping") {
    return "border-foreground/20 bg-foreground/5";
  }
  return "border-muted-foreground/40 bg-muted/40 text-muted-foreground hover:border-foreground/50 hover:bg-muted/60 hover:text-foreground";
}

function connectionProfileLabel(profile: ConnectionProfile): string {
  if (profile.kind === "manual") return profileLabel(profile.outbound);
  return profile.kind === "subscription" ? profile.label : profile.name;
}

export type IndexedHomeProfile = { index: number; profile: ConnectionProfile };
export type HomeSubscriptionGroup = { id: string; rows: IndexedHomeProfile[] };
export type GroupedHomeProfiles = {
  manual: IndexedHomeProfile[];
  subscriptions: HomeSubscriptionGroup[];
};

export function subscriptionGroupLabel(
  subscriptionId: string,
  subscriptionNames: ReadonlyMap<string, string>,
): string {
  const name = subscriptionNames.get(subscriptionId)?.trim();
  return name || "Subscription";
}

export function groupHomeProfiles(profiles: ConnectionProfile[]): GroupedHomeProfiles {
  const manual: IndexedHomeProfile[] = [];
  const subscriptions: HomeSubscriptionGroup[] = [];
  const byId = new Map<string, HomeSubscriptionGroup>();

  profiles.forEach((profile, index) => {
    if (profile.kind === "manual") {
      manual.push({ index, profile });
      return;
    }

    const id = profile.kind === "subscription"
      ? profile.reference.subscription_id
      : profile.subscriptionId;
    let group = byId.get(id);
    if (!group) {
      group = { id, rows: [] };
      byId.set(id, group);
      subscriptions.push(group);
    }
    group.rows.push({ index, profile });
  });

  return { manual, subscriptions };
}

export function connectionProfileDisplay(
  profile: ConnectionProfile,
  readyProfileMetadata: ReadonlyMap<string, HomeProfileMetadata>,
  geoipByIp: Record<string, string>,
  latencyByTag: Map<string, number>,
): { flag: string; code: string; label: string; protocol: string; ms: number | undefined; key: string } {
  if (profile.kind === "subscription") {
    return {
      flag: "🌐",
      code: "??",
      label: profile.label,
      protocol: profile.protocol,
      ms: undefined,
      key: `${profile.reference.subscription_id}-${profile.reference.link_key}`,
    };
  }
  if (profile.kind === "ready_config") {
    const metadata = profile.engine === "xray"
      ? readyProfileMetadata.get(`${profile.subscriptionId}:${profile.key}`)
      : undefined;
    const code = metadata?.country_code ?? "??";
    return {
      flag: code === "??" ? "🌐" : code,
      code,
      label: profile.name,
      protocol: profile.engine === "singbox" ? "sing-box" : "Xray",
      ms: metadata?.latency_ms ?? undefined,
      key: `${profile.subscriptionId}-${profile.key}`,
    };
  }
  const outbound = profile.outbound;
  const supported = isSupported(outbound);
  const flag = flagForProfile({
    tag: supported ? outbound.tag : undefined,
    server: supported ? outbound.server : undefined,
    geoipByIp,
  });
  return {
    ...flag,
    label: profileLabel(outbound),
    protocol: outbound.protocol,
    ms: supported ? latencyByTag.get(outbound.tag) : undefined,
    key: profileEndpoint(outbound) || outbound.protocol,
  };
}

function ProfileChoice({
  row,
  selectedIndex,
  readyProfileMetadata,
  geoipByIp,
  latencyByTag,
  onSelect,
  onSelectionDone,
  compact,
}: {
  row: IndexedHomeProfile;
  selectedIndex: number;
  readyProfileMetadata: ReadonlyMap<string, HomeProfileMetadata>;
  geoipByIp: Record<string, string>;
  latencyByTag: Map<string, number>;
  onSelect: (index: number) => void;
  onSelectionDone?: () => void;
  compact: boolean;
}) {
  const display = connectionProfileDisplay(row.profile, readyProfileMetadata, geoipByIp, latencyByTag);
  return (
    <button
      type="button"
      onClick={() => {
        onSelect(row.index);
        onSelectionDone?.();
      }}
      className={cn(
        "flex w-full items-center gap-2 rounded text-left text-xs transition-colors",
        compact ? "px-2 py-1.5" : "border border-border bg-card/30 px-2 py-1.5 hover:bg-accent",
        row.index === selectedIndex
          ? compact ? "bg-foreground/5" : "border-foreground/30 bg-foreground/5"
          : compact ? "hover:bg-accent" : "",
      )}
    >
      <FlagIcon code={display.code} size={compact ? 16 : 14} className="shrink-0 self-center" />
      <span className="min-w-0 flex-1 truncate font-medium">{display.label}</span>
      {compact && <span className="font-mono text-[10px] text-muted-foreground">{display.protocol}</span>}
      <LatencyBadge ms={display.ms} />
    </button>
  );
}

function GroupedHomeProfileRows({
  profiles,
  selectedIndex,
  readyProfileMetadata,
  subscriptionNames,
  geoipByIp,
  latencyByTag,
  onSelect,
  onSelectionDone,
  mode,
}: {
  profiles: ConnectionProfile[];
  selectedIndex: number;
  readyProfileMetadata: ReadonlyMap<string, HomeProfileMetadata>;
  subscriptionNames: ReadonlyMap<string, string>;
  geoipByIp: Record<string, string>;
  latencyByTag: Map<string, number>;
  onSelect: (index: number) => void;
  onSelectionDone?: () => void;
  mode: "picker" | "grid";
}) {
  const grouped = groupHomeProfiles(profiles);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const selected = profiles[selectedIndex];
  const selectedSubscriptionId = selected?.kind === "subscription"
    ? selected.reference.subscription_id
    : selected?.kind === "ready_config"
      ? selected.subscriptionId
      : null;

  useEffect(() => {
    if (selectedSubscriptionId) {
      setExpanded((current) => current.has(selectedSubscriptionId) ? current : new Set([selectedSubscriptionId, ...current]));
    }
  }, [selectedSubscriptionId]);

  const renderRows = (rows: IndexedHomeProfile[]) => rows.map((row) => (
    <li key={`${row.index}-${connectionProfileDisplay(row.profile, readyProfileMetadata, geoipByIp, latencyByTag).key}`}>
      <ProfileChoice
        row={row}
        selectedIndex={selectedIndex}
        readyProfileMetadata={readyProfileMetadata}
        geoipByIp={geoipByIp}
        latencyByTag={latencyByTag}
        onSelect={onSelect}
        onSelectionDone={onSelectionDone}
        compact={mode === "picker"}
      />
    </li>
  ));

  const content = (
    <>
      {grouped.manual.length > 0 && (
        <li>
          <div className="px-2 pb-1 pt-1 text-[10px] uppercase tracking-wider text-muted-foreground">Manual servers</div>
          {mode === "grid" ? (
            <div className="grid grid-cols-2 gap-1.5 sm:grid-cols-3">
              {grouped.manual.map((row) => (
                <ProfileChoice key={row.index} row={row} selectedIndex={selectedIndex} readyProfileMetadata={readyProfileMetadata} geoipByIp={geoipByIp} latencyByTag={latencyByTag} onSelect={onSelect} onSelectionDone={onSelectionDone} compact={false} />
              ))}
            </div>
          ) : <ul>{renderRows(grouped.manual)}</ul>}
        </li>
      )}
      {grouped.subscriptions.map((group) => {
        const isExpanded = expanded.has(group.id);
        return (
          <li key={group.id}>
            <button
              type="button"
              onClick={() => setExpanded((current) => {
                const next = new Set(current);
                if (next.has(group.id)) next.delete(group.id); else next.add(group.id);
                return next;
              })}
              className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[10px] uppercase tracking-wider text-muted-foreground hover:bg-accent"
              aria-expanded={isExpanded}
            >
              <ChevronDown className={cn("h-3 w-3 transition-transform", !isExpanded && "-rotate-90")} />
              <span className="min-w-0 flex-1 truncate">{subscriptionGroupLabel(group.id, subscriptionNames)}</span>
              <span className="font-mono normal-case">{group.rows.length}</span>
            </button>
            {isExpanded && (mode === "grid" ? (
              <div className="grid grid-cols-2 gap-1.5 sm:grid-cols-3">
                {group.rows.map((row) => (
                  <ProfileChoice key={row.index} row={row} selectedIndex={selectedIndex} readyProfileMetadata={readyProfileMetadata} geoipByIp={geoipByIp} latencyByTag={latencyByTag} onSelect={onSelect} onSelectionDone={onSelectionDone} compact={false} />
                ))}
              </div>
            ) : <ul>{renderRows(group.rows)}</ul>)}
          </li>
        );
      })}
    </>
  );

  return mode === "grid" ? <div className="space-y-2">{content}</div> : content;
}

/**
 * Compact dropdown-style server picker rendered above the hero icon.
 * Click → menu with the full list; click outside → closes.
 */
function ServerPicker({
  profiles,
  selectedIndex,
  latencyByTag,
  readyProfileMetadata,
  subscriptionNames,
  geoipByIp,
  onSelect,
}: {
  profiles: ConnectionProfile[];
  selectedIndex: number;
  latencyByTag: Map<string, number>;
  readyProfileMetadata: ReadonlyMap<string, HomeProfileMetadata>;
  subscriptionNames: ReadonlyMap<string, string>;
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
  const selectedDisplay = selected
    ? connectionProfileDisplay(selected, readyProfileMetadata, geoipByIp, latencyByTag)
    : null;
  const selectedFlag = selectedDisplay ?? { flag: "🌐", code: "??", label: "No servers", ms: undefined, key: "none", protocol: "" };
  const selectedName = selectedDisplay?.label ?? "No servers";
  const selectedMs = selectedDisplay?.ms;

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
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
            <GroupedHomeProfileRows
              profiles={profiles}
              selectedIndex={selectedIndex}
              readyProfileMetadata={readyProfileMetadata}
              subscriptionNames={subscriptionNames}
              geoipByIp={geoipByIp}
              latencyByTag={latencyByTag}
              onSelect={onSelect}
              onSelectionDone={() => setOpen(false)}
              mode="picker"
            />
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
