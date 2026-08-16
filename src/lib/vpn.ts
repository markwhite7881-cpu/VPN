// Bridge to the Android VPN plugin (Kotlin VpnService + libbox) and
// to the shared traffic commands. Desktop never imports this module —
// it drives the sing-box sidecar through `lib/api.ts` instead.
//
// The plugin commands are registered by the Kotlin side as
// `plugin:vpn|<command>`; the status event is emitted as "status".

import {
  addPluginListener,
  invoke,
  type PluginListener,
} from "@tauri-apps/api/core";

export type VpnState = "stopped" | "starting" | "running" | "error";

export interface VpnStatus {
  state: VpnState;
  message: string | null;
  since: number | null;
}

export interface AppEntry {
  label: string;
  packageName: string;
  system: boolean;
  hasInternet: boolean;
}

/** Ask the OS for VPN permission. Resolves `{ prepared: false }`
 *  when the user declined (or the dialog is still pending). */
export const vpnPrepare = () =>
  invoke<{ prepared: boolean }>("plugin:vpn|prepare");

/** Start the tunnel with a full sing-box config (JSON string). */
export const vpnStart = (config: string) =>
  invoke<void>("plugin:vpn|start", { config });

export const vpnStop = () => invoke<void>("plugin:vpn|stop");

export const vpnStatus = () => invoke<VpnStatus>("plugin:vpn|status");

/** Installed apps for the per-app routing picker.
 *  Kotlin resolves `{ apps: [...] }` — unwrap it here. */
export const vpnListApps = async (): Promise<AppEntry[]> => {
  const r = await invoke<{ apps?: AppEntry[] } | AppEntry[]>(
    "plugin:vpn|listApps",
  );
  if (Array.isArray(r)) return r;
  return r?.apps ?? [];
};

/** sing-box core version embedded in libbox (e.g. "1.14.0").
 *  Kotlin resolves `{ value: "..." }` — unwrap it here. */
export const vpnCoreVersion = async (): Promise<string> => {
  const r = await invoke<string | { value?: string }>(
    "plugin:vpn|coreVersion",
  );
  return typeof r === "string" ? r : (r?.value ?? "");
};

/** Tail of the core log ring buffer, one line per entry.
 *  Kotlin resolves `{ value: "..." }` — unwrap it here. */
export const vpnReadLogs = async (maxLines = 300): Promise<string> => {
  const r = await invoke<string | { value?: string }>("plugin:vpn|readLogs", {
    maxLines,
  });
  return typeof r === "string" ? r : (r?.value ?? "");
};

/** Subscribe to VPN state changes pushed by the service. */
export const onVpnStatus = (
  cb: (s: VpnStatus) => void,
): Promise<PluginListener> => addPluginListener("vpn", "status", cb);

// --- Shared Rust commands (exist on desktop too, live in commands.rs) ---

/** Point the Clash API helper at the running core's controller.
 *  Pass `null` on disconnect so `list_proxies`/`start_traffic` stop
 *  hitting a dead listener. */
export const setControllerUrl = (url: string | null) =>
  invoke<void>("set_controller_url", { url });

/** Start polling the Clash API and emitting "traffic" events. */
export const startTraffic = () => invoke<void>("start_traffic");

export const stopTraffic = () => invoke<void>("stop_traffic");
