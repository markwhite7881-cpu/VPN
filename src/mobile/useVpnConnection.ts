import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "@/lib/api";
import {
  onVpnStatus,
  setControllerUrl,
  startTraffic,
  stopTraffic,
  vpnPrepare,
  vpnStart,
  vpnStatus,
  vpnStop,
  type VpnState,
  type VpnStatus,
} from "@/lib/vpn";
import type { GeneratorSettings, Outbound } from "@/lib/types";

const inTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export interface VpnConnection {
  /** Last known VPN state (drives the button + status dot). */
  state: VpnState;
  /** Error/info message from the service (state === "error"). */
  message: string | null;
  /** Epoch ms when the current state began (for uptime). */
  since: number | null;
  /** False until the initial `vpnStatus()` call resolves. */
  ready: boolean;
  /** True while a connect/disconnect action is in flight. */
  busy: boolean;
  /** Human-readable failure of the last action, if any. */
  error: string | null;
  connect: () => Promise<void>;
  disconnect: () => Promise<void>;
}

function humanError(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}

/**
 * Mobile connect flow — mirrors the desktop `handleConnect`, but the
 * core is started by the Kotlin VpnService (via the vpn plugin)
 * instead of a sidecar process:
 *
 *   1. vpnPrepare()      — OS VPN permission dialog (first run).
 *   2. generate_config   — same Rust command the desktop uses.
 *   3. vpnStart(json)    — plugin starts libbox with the config.
 *   4. set_controller_url + start_traffic — point the shared Clash
 *      API helper at the now-running core and begin the 1 Hz
 *      "traffic" event stream.
 *
 * Disconnect reverses it: vpnStop, stop_traffic, clear the URL.
 */
export function useVpnConnection(
  profiles: Outbound[],
  settings: GeneratorSettings,
): VpnConnection {
  const [status, setStatus] = useState<VpnStatus>({
    state: "stopped",
    message: null,
    since: null,
  });
  const [ready, setReady] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Latest profiles/settings for the async connect closure.
  const profilesRef = useRef(profiles);
  const settingsRef = useRef(settings);
  useEffect(() => {
    profilesRef.current = profiles;
    settingsRef.current = settings;
  }, [profiles, settings]);

  // Initial status (survives activity recreation — the service keeps
  // running when the WebView is torn down) + live subscription.
  useEffect(() => {
    if (!inTauri) {
      setReady(true);
      return;
    }
    let cancelled = false;
    let listener: { unregister: () => Promise<void> } | null = null;
    (async () => {
      try {
        const s = await vpnStatus();
        if (!cancelled) setStatus(s);
      } catch (e) {
        if (!cancelled) setError(humanError(e));
      } finally {
        if (!cancelled) setReady(true);
      }
      try {
        const l = await onVpnStatus((s) => setStatus(s));
        if (cancelled) {
          void l.unregister();
        } else {
          listener = l;
        }
      } catch (e) {
        if (!cancelled) setError(humanError(e));
      }
    })();
    return () => {
      cancelled = true;
      if (listener) void listener.unregister();
    };
  }, []);

  const connect = useCallback(async () => {
    setError(null);
    // Browser preview (?mobile=1 outside Tauri): fake the state so
    // the UI is demoable.
    if (!inTauri) {
      setBusy(true);
      setStatus({ state: "starting", message: null, since: Date.now() });
      await new Promise((r) => setTimeout(r, 600));
      setStatus({ state: "running", message: null, since: Date.now() });
      setBusy(false);
      return;
    }
    if (profilesRef.current.length === 0) {
      setError("Add a server first (Servers tab).");
      return;
    }
    setBusy(true);
    try {
      // 1. VPN permission (Android shows the system dialog on the
      //    first call; resolves prepared=false when declined).
      const { prepared } = await vpnPrepare();
      if (!prepared) {
        setError("VPN permission required.");
        return;
      }
      // 2. Fresh config from the current profiles + settings — the
      //    same generator the desktop uses.
      const config = await api.generateConfig(
        profilesRef.current,
        settingsRef.current,
      );
      // 3. Hand the config to the VpnService.
      await vpnStart(JSON.stringify(config));
      // 4. Point the shared Clash API helper at the core and start
      //    the traffic event stream. Best-effort: the tunnel is up
      //    even if the controller port isn't reachable yet.
      const controller = `http://${settingsRef.current.clash_api.external_controller || "127.0.0.1:9090"}`;
      try {
        await setControllerUrl(controller);
        await startTraffic();
      } catch {
        /* controller not ready yet — the next status event still drives the UI */
      }
    } catch (e) {
      setError(humanError(e));
    } finally {
      setBusy(false);
    }
  }, []);

  const disconnect = useCallback(async () => {
    setError(null);
    if (!inTauri) {
      setStatus({ state: "stopped", message: null, since: Date.now() });
      return;
    }
    setBusy(true);
    try {
      await vpnStop();
      try {
        await stopTraffic();
        await setControllerUrl(null);
      } catch {
        /* core already gone — nothing to drain */
      }
    } catch (e) {
      setError(humanError(e));
    } finally {
      setBusy(false);
    }
  }, []);

  return {
    state: status.state,
    message: status.message,
    since: status.since,
    ready,
    busy,
    error,
    connect,
    disconnect,
  };
}
