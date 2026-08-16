import { useEffect, useState } from "react";
import {
  Loader2,
  Power,
  TrendingDown,
  TrendingUp,
} from "lucide-react";
import { FlagIcon } from "@/components/FlagIcon";
import { useTrafficStream } from "@/hooks/useTrafficStream";
import { flagForProfile } from "@/lib/flags";
import { isSupported, profileEndpoint, profileLabel } from "@/lib/outbound";
import { cn } from "@/lib/utils";
import type { GeneratorSettings, Outbound } from "@/lib/types";
import type { VpnConnection } from "../useVpnConnection";
import { summarizeRoutingPolicy } from "../lib/mobileUi";
import { SectionCard } from "../components/SectionCard";
import { formatRate, formatBytes, formatUptime } from "../lib/format";

const inTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function HomeScreen({
  vpn,
  profiles,
  selectedIndex,
  geoipByIp,
  settings,
  onOpenServers,
  onOpenRouting,
}: {
  vpn: VpnConnection;
  profiles: Outbound[];
  selectedIndex: number;
  geoipByIp: Record<string, string>;
  settings: GeneratorSettings;
  onOpenServers: () => void;
  onOpenRouting: () => void;
}) {
  const isRunning = vpn.state === "running";
  const isTransition = vpn.state === "starting";
  const traffic = useTrafficStream(isRunning || !inTauri, profiles.length);
  const current = traffic.current;

  // 1 Hz ticker so the uptime counter advances while connected.
  const [, setTick] = useState(0);
  useEffect(() => {
    if (!isRunning) return;
    const t = window.setInterval(() => setTick((v) => v + 1), 1000);
    return () => window.clearInterval(t);
  }, [isRunning]);

  const selected =
    selectedIndex >= 0 && selectedIndex < profiles.length
      ? profiles[selectedIndex]
      : undefined;
  const selectedSupported = selected && isSupported(selected) ? selected : null;
  const flag = selectedSupported
    ? flagForProfile({
        tag: selectedSupported.tag,
        server: selectedSupported.server,
        geoipByIp,
      })
    : null;

  const headline =
    vpn.state === "running"
      ? "Connected"
      : vpn.state === "starting"
        ? "Connecting…"
        : vpn.state === "error"
          ? "Error"
          : "Disconnected";

  const serverLine = !selectedSupported
    ? "Auto (best latency)"
    : profileLabel(selectedSupported);
  const routingSummary = summarizeRoutingPolicy(
    settings.routing.final_outbound,
    settings.routing.tun_app_mode,
    settings.routing.tun_app_list ?? [],
  );

  const uptimeSecs =
    isRunning && vpn.since
      ? Math.max(0, Math.floor((Date.now() - vpn.since) / 1000))
      : null;

  return (
    <div className="flex flex-col gap-4 p-4">
      {/* Hero: connect button + state. */}
      <SectionCard className="flex flex-col items-center gap-5 px-4 py-8 text-center">
        <button
          type="button"
          onClick={() => {
            if (isRunning) void vpn.disconnect();
            else void vpn.connect();
          }}
          disabled={vpn.busy || isTransition || !vpn.ready}
          aria-label={isRunning ? "Disconnect" : "Connect"}
          className={cn(
            "group relative flex h-28 w-28 items-center justify-center rounded-full",
            "border-2 transition-all duration-200",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-foreground/40",
            "disabled:cursor-not-allowed",
            isRunning
              ? "border-emerald-400/60 bg-emerald-500/15 text-emerald-300 shadow-[0_0_50px_rgba(52,211,153,0.22)] ring-4 ring-emerald-400/10 hover:bg-emerald-500/25"
              : isTransition
                ? "border-foreground/20 bg-foreground/5"
                : "border-muted-foreground/40 bg-muted/40 text-muted-foreground hover:border-foreground/50 hover:text-foreground",
          )}
        >
          {isTransition ? (
            <Loader2 className="h-11 w-11 animate-spin text-foreground/70" />
          ) : (
            <Power
              className={cn(
                "h-11 w-11 transition-transform group-hover:scale-110",
                isRunning &&
                  "text-emerald-300 drop-shadow-[0_0_8px_rgba(52,211,153,0.5)]",
              )}
            />
          )}
        </button>

        <div className="space-y-1">
          <h1 className="text-xl font-semibold tracking-tight">{headline}</h1>
          <button
            type="button"
            onClick={onOpenServers}
            className="flex items-center justify-center gap-1.5 text-sm text-muted-foreground"
          >
            {flag && <FlagIcon code={flag.code} size={14} className="self-center" />}
            <span className="max-w-[240px] truncate">{serverLine}</span>
          </button>
          {vpn.state === "error" && vpn.message && (
            <p className="max-w-[280px] text-xs text-destructive">{vpn.message}</p>
          )}
          {vpn.error && (
            <p className="max-w-[280px] text-xs text-destructive">{vpn.error}</p>
          )}
        </div>
      </SectionCard>

      <SectionCard className="grid divide-y divide-border overflow-hidden sm:grid-cols-2 sm:divide-x sm:divide-y-0">
        <button
          type="button"
          onClick={onOpenServers}
          className="min-w-0 px-3.5 py-3 text-left hover:bg-accent"
        >
          <p className="text-[10px] uppercase tracking-wider text-muted-foreground">Server</p>
          <p className="mt-1 flex items-center gap-1.5 truncate text-sm font-medium">
            {flag && <FlagIcon code={flag.code} size={14} className="shrink-0" />}
            <span className="truncate">{serverLine}</span>
          </p>
        </button>
        <button
          type="button"
          onClick={onOpenRouting}
          className="min-w-0 px-3.5 py-3 text-left hover:bg-accent"
        >
          <p className="text-[10px] uppercase tracking-wider text-muted-foreground">Routing</p>
          <p className="mt-1 truncate text-sm font-medium" title={routingSummary}>{routingSummary}</p>
        </button>
      </SectionCard>

      {/* Live traffic. */}
      <div className="grid grid-cols-2 gap-3">
        <SectionCard className="p-3.5">
          <div className="flex items-center gap-1.5 text-[10px] uppercase tracking-wider text-emerald-400/90">
            <TrendingDown className="h-3 w-3" />
            Download
          </div>
          <p className="mt-1 font-mono text-xl font-semibold tabular-nums text-emerald-300">
            {formatRate(current?.down_bps ?? 0)}
          </p>
          <p className="mt-0.5 font-mono text-[10px] text-emerald-400/60">
            total {formatBytes(current?.down_total ?? 0)}
          </p>
        </SectionCard>
        <SectionCard className="p-3.5">
          <div className="flex items-center gap-1.5 text-[10px] uppercase tracking-wider text-amber-400/90">
            <TrendingUp className="h-3 w-3" />
            Upload
          </div>
          <p className="mt-1 font-mono text-xl font-semibold tabular-nums text-amber-300">
            {formatRate(current?.up_bps ?? 0)}
          </p>
          <p className="mt-0.5 font-mono text-[10px] text-amber-400/60">
            total {formatBytes(current?.up_total ?? 0)}
          </p>
        </SectionCard>
      </div>

      {/* Session details. */}
      <SectionCard className="grid grid-cols-3 gap-3 p-3.5 text-center">
        <Metric label="Uptime" value={formatUptime(uptimeSecs)} mono />
        <Metric
          label="Server"
          value={selectedSupported ? profileEndpoint(selectedSupported) : "auto"}
          mono
        />
        <Metric
          label="Protocol"
          value={selectedSupported ? selectedSupported.protocol : "—"}
        />
      </SectionCard>
    </div>
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
          "truncate text-xs",
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
