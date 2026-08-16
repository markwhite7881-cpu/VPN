import { useEffect, useMemo, useRef, useState, type TouchEvent } from "react";
import {
  Home,
  Link2,
  RefreshCw,
  Route,
  Settings2,
} from "lucide-react";
import cloakwireLogo from "@/assets/cloakwire-logo.png";
import { useGeoIp } from "@/hooks/useGeoIp";
import { useSubscriptions } from "@/hooks/useSubscriptions";
import { loadManualProfiles, saveManualProfiles } from "@/lib/manualProfiles";
import { isSupported } from "@/lib/outbound";
import { cn } from "@/lib/utils";
import type { GeneratorSettings, Outbound } from "@/lib/types";
import {
  loadAutoConnect,
  loadSettings,
  saveAutoConnect,
  saveSettings,
} from "./lib/settings";
import { useVpnConnection } from "./useVpnConnection";
import { HomeScreen } from "./screens/HomeScreen";
import { ServersScreen } from "./screens/ServersScreen";
import { RoutingScreen } from "./screens/RoutingScreen";
import { LogsScreen } from "./screens/LogsScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { adjacentTabIndex, swipeDirection } from "./lib/mobileUi";

type TouchStart = {
  x: number;
  y: number;
  target: EventTarget | null;
};
const TABS = [
  { id: "home", label: "Home", icon: Home },
  { id: "servers", label: "Servers", icon: Link2 },
  { id: "routing", label: "Routing", icon: Route },
  { id: "settings", label: "Settings", icon: Settings2 },
] as const;
type TabId = (typeof TABS)[number]["id"];

const TAB_KEY = "singbox.mobile.tab";

function readStoredTab(): TabId {
  try {
    const v = window.localStorage.getItem(TAB_KEY);
    if (v && (TABS as readonly { id: string }[]).some((t) => t.id === v)) {
      return v as TabId;
    }
  } catch {
    /* storage disabled — fall through */
  }
  return "home";
}

export default function MobileApp() {
  const [activeTab, setActiveTab] = useState<TabId>(readStoredTab);
  const [logsOpen, setLogsOpen] = useState(false);
  const [transitionDirection, setTransitionDirection] = useState<"previous" | "next">("next");
  const [settings, setSettings] = useState<GeneratorSettings>(loadSettings);
  const [autoConnect, setAutoConnect] = useState<boolean>(loadAutoConnect);
  const [manualProfiles, setManualProfiles] = useState<Outbound[]>(() =>
    loadManualProfiles(),
  );
  const [selectedIndex, setSelectedIndex] = useState<number>(0);
  const [connectionDirty, setConnectionDirty] = useState(false);
  const reconnectAttempted = useRef(false);
  const contentRef = useRef<HTMLElement | null>(null);
  const touchStart = useRef<TouchStart | null>(null);

  const subs = useSubscriptions();

  useEffect(() => saveSettings(settings), [settings]);
  useEffect(() => saveAutoConnect(autoConnect), [autoConnect]);
  useEffect(() => saveManualProfiles(manualProfiles), [manualProfiles]);
  useEffect(() => {
    try {
      window.localStorage.setItem(TAB_KEY, activeTab);
    } catch {
      /* ignore */
    }
  }, [activeTab]);

  // Flattened + deduplicated profiles — same rule as the desktop:
  // drop repeated endpoints, disambiguate repeated tags.
  const profiles = useMemo<Outbound[]>(() => {
    const raw: Outbound[] = [...manualProfiles];
    for (const s of subs.subs) {
      const r = subs.lastResult[s.id];
      if (r) raw.push(...r.outbounds);
    }
    const seenEndpoint = new Set<string>();
    const seenTag = new Set<string>();
    const out: Outbound[] = [];
    for (const p of raw) {
      if (p.protocol === "unsupported") {
        out.push(p);
        continue;
      }
      const endpoint = `${p.server}:${p.port}`;
      if (seenEndpoint.has(endpoint)) continue;
      if (seenTag.has(p.tag)) {
        out.push({ ...p, tag: `${p.tag} @${endpoint}` });
      } else {
        out.push(p);
        seenTag.add(p.tag);
      }
      seenEndpoint.add(endpoint);
    }
    return out;
  }, [manualProfiles, subs.subs, subs.lastResult]);

  const geoip = useGeoIp(profiles);
  const vpn = useVpnConnection(profiles, settings);

  const markConnectionDirty = () => {
    if (vpn.state === "running") setConnectionDirty(true);
  };

  const setMobileSettings = (next: GeneratorSettings) => {
    setSettings(next);
    markConnectionDirty();
  };

  const reconnect = async () => {
    if (vpn.busy || vpn.state !== "running") return;
    reconnectAttempted.current = true;
    await vpn.disconnect();
    await new Promise((resolve) => window.setTimeout(resolve, 50));
    await vpn.connect();
  };

  useEffect(() => {
    if (reconnectAttempted.current && vpn.state === "running") {
      setConnectionDirty(false);
      reconnectAttempted.current = false;
    }
  }, [vpn.state]);

  const changeTab = (nextTab: TabId, direction?: "previous" | "next") => {
    if (nextTab === activeTab) return;
    const currentIndex = TABS.findIndex((tab) => tab.id === activeTab);
    const nextIndex = TABS.findIndex((tab) => tab.id === nextTab);
    if (currentIndex < 0 || nextIndex < 0) return;
    setTransitionDirection(direction ?? (nextIndex > currentIndex ? "next" : "previous"));
    setLogsOpen(false);
    setActiveTab(nextTab);
  };

  const onTouchStart = (event: TouchEvent<HTMLElement>) => {
    const touch = event.touches[0];
    if (!touch) return;
    touchStart.current = { x: touch.clientX, y: touch.clientY, target: event.target };
  };

  const onTouchEnd = (event: TouchEvent<HTMLElement>) => {
    const start = touchStart.current;
    touchStart.current = null;
    const touch = event.changedTouches[0];
    if (!start || !touch) return;
    const direction = swipeDirection({
      dx: touch.clientX - start.x,
      dy: touch.clientY - start.y,
      startTarget: start.target,
      allowControls: activeTab === "settings" && !logsOpen,
      scrollContainer: activeTab === "settings" && !logsOpen ? contentRef.current : null,
    });
    if (!direction) return;
    const currentIndex = TABS.findIndex((tab) => tab.id === activeTab);
    const nextIndex = adjacentTabIndex(currentIndex, direction, TABS.length);
    if (nextIndex !== currentIndex) changeTab(TABS[nextIndex].id, direction);
  };

  // Auto-connect on launch (opt-in from Settings). Fires once the
  // initial vpnStatus() has resolved so we don't fight a tunnel
  // that's already up.
  const autoConnectFired = useRef(false);
  useEffect(() => {
    if (
      autoConnect &&
      vpn.ready &&
      !autoConnectFired.current &&
      vpn.state === "stopped" &&
      profiles.length > 0
    ) {
      autoConnectFired.current = true;
      void vpn.connect();
    }
  }, [autoConnect, vpn.ready, vpn.state, profiles.length, vpn.connect]);

  // Clamp the selection when the list shrinks.
  useEffect(() => {
    if (profiles.length === 0 && selectedIndex !== -1) setSelectedIndex(-1);
    else if (
      profiles.length > 0 &&
      (selectedIndex < 0 || selectedIndex >= profiles.length)
    ) {
      setSelectedIndex(0);
    }
  }, [profiles.length, selectedIndex]);

  const onSelectProfile = (index: number) => {
    const isAuto = index === -1;
    let pickedTag: string | null = null;
    if (!isAuto) {
      const p = profiles[index];
      if (!p || !isSupported(p)) return;
      pickedTag = p.tag;
    }
    setSelectedIndex(index);
    setMobileSettings({ ...settings, default_outbound: pickedTag });
  };

  const dotCls =
    vpn.state === "running"
      ? "bg-emerald-400"
      : vpn.state === "starting"
        ? "bg-foreground animate-pulse-dot"
        : vpn.state === "error"
          ? "bg-red-400"
          : "bg-muted-foreground";

  return (
    <div className="relative flex h-screen flex-col overflow-hidden bg-background text-foreground">
      {/* Header: brand + live status dot. */}
      <header className="flex shrink-0 items-center justify-between border-b border-border bg-card/40 px-4 py-2.5 backdrop-blur pt-[max(0.625rem,env(safe-area-inset-top))]">
        <div className="flex items-center gap-2.5">
          <div className="flex h-7 w-7 items-center justify-center overflow-hidden rounded-md bg-primary/15 ring-1 ring-primary/30">
            <img src={cloakwireLogo} alt="Cloakwire" className="h-5 w-5" />
          </div>
          <h1 className="text-sm font-semibold tracking-tight">Cloakwire</h1>
        </div>
        <div className="flex items-center gap-1.5">
          <span className={cn("h-2 w-2 rounded-full", dotCls)} />
          <span className="text-[11px] text-muted-foreground">
            {vpn.state === "running"
              ? "Connected"
              : vpn.state === "starting"
                ? "Connecting"
                : vpn.state === "error"
                  ? "Error"
                  : "Offline"}
          </span>
        </div>
      </header>

      {/* Content */}
      <main
        ref={contentRef}
        className="min-h-0 flex-1 overflow-y-auto"
        onTouchStart={onTouchStart}
        onTouchEnd={onTouchEnd}
      >
        {connectionDirty && vpn.state === "running" && (
          <div className="sticky top-0 z-10 flex items-center justify-between gap-3 border-b border-border bg-card px-4 py-2 shadow-sm">
            <span className="text-xs text-muted-foreground">Settings changed</span>
            <button
              type="button"
              onClick={() => void reconnect()}
              disabled={vpn.busy}
              className="flex h-8 items-center gap-1.5 rounded-md bg-primary px-2.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            >
              <RefreshCw className={cn("h-3.5 w-3.5", vpn.busy && "animate-spin")} />
              Reconnect
            </button>
          </div>
        )}
        <div
          key={`${activeTab}:${logsOpen ? "logs" : "main"}`}
          className={`mobile-view-enter-${transitionDirection}`}
        >
          {activeTab === "home" && (
            <HomeScreen
              vpn={vpn}
              profiles={profiles}
              selectedIndex={selectedIndex}
              geoipByIp={geoip.byIp}
              settings={settings}
              onOpenServers={() => changeTab("servers")}
              onOpenRouting={() => changeTab("routing")}
            />
          )}
          {activeTab === "servers" && (
            <ServersScreen
              profiles={profiles}
              selectedIndex={selectedIndex}
              geoipByIp={geoip.byIp}
              onSelect={onSelectProfile}
              subs={subs.subs}
              subFetching={subs.fetching}
              onAddSub={(input) => {
                subs.add(input);
                markConnectionDirty();
              }}
              onAddLinks={(obs) => {
                setManualProfiles((prev) => [...obs, ...prev]);
                markConnectionDirty();
              }}
              onRemoveSub={(id) => {
                subs.remove(id);
                markConnectionDirty();
              }}
              onRefreshSub={(id) => {
                void subs.refreshOne(id);
                markConnectionDirty();
              }}
            />
          )}
          {activeTab === "routing" && (
            <RoutingScreen settings={settings} onSettingsChange={setMobileSettings} />
          )}
          {activeTab === "settings" && logsOpen && (
            <LogsScreen
              onBack={() => {
                setTransitionDirection("previous");
                setLogsOpen(false);
              }}
            />
          )}
          {activeTab === "settings" && !logsOpen && (
            <SettingsScreen
              settings={settings}
              onSettingsChange={setMobileSettings}
              autoConnect={autoConnect}
              onAutoConnectChange={setAutoConnect}
              onRefreshAllSubs={() => {
                void subs.refreshAll();
                markConnectionDirty();
              }}
              subsFetching={Object.values(subs.fetching).some(Boolean)}
              onOpenLogs={() => {
                setTransitionDirection("next");
                setLogsOpen(true);
              }}
            />
          )}
        </div>
      </main>

      {/* Bottom navigation. */}
      <nav className="shrink-0 border-t border-border bg-card/60 backdrop-blur pb-[env(safe-area-inset-bottom)]">
        <div className="flex items-stretch">
          {TABS.map((t) => {
            const Icon = t.icon;
            const active = t.id === activeTab;
            return (
              <button
                key={t.id}
                type="button"
                onClick={() => changeTab(t.id)}
                aria-current={active ? "page" : undefined}
                className={cn(
                  "flex flex-1 flex-col items-center gap-0.5 py-2 transition-colors",
                  active
                    ? "text-foreground"
                    : "text-muted-foreground hover:text-foreground/80",
                )}
              >
                <Icon className="h-5 w-5" />
                <span className="text-[10px] font-medium">{t.label}</span>
              </button>
            );
          })}
        </div>
      </nav>
    </div>
  );
}
