/**
 * Typed wrappers around the Tauri command surface.
 *
 * The Rust side serialises AppError as `{ kind, message }`. We surface
 * that as a JS Error so `try/catch` still works in the React code.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  AddSubscriptionInput,
  BinaryInfo,
  ManagedLaunchResult,
  SubscriptionLinkRef,
  SubscriptionSnapshot,
  RefreshSubscriptionResult,
  SubscriptionSummary,
  GeneratorSettings,
  HomeProfileMetadata,
  LogLine,
  Outbound,
  ParseLinksResult,
  ParsedInput,
  ProcessInfo,
  ProxiesResponse,
  SingboxVersion,
  StatusReport,
} from "./types";

class TauriCommandError extends Error {
  constructor(public kind: string, message: string) {
    super(message);
    this.name = "TauriCommandError";
  }
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    if (
      e &&
      typeof e === "object" &&
      "kind" in e &&
      "message" in e &&
      typeof (e as { kind: unknown }).kind === "string"
    ) {
      const { kind, message } = e as { kind: string; message: string };
      throw new TauriCommandError(kind, message);
    }
    throw e;
  }
}

export const api = {
  ping: () => call<string>("ping"),

  getBinaryInfo: () => call<BinaryInfo>("get_binary_info"),
  getSingboxVersion: () => call<SingboxVersion>("get_singbox_version"),
  checkConfig: (configPath: string) =>
    call<string>("check_config", { configPath }),

  startConnection: (configPath: string, controllerUrl?: string) =>
    call<StatusReport>("start_connection", { configPath, controllerUrl }),
  stopConnection: () => call<StatusReport>("stop_connection"),
  start: (configPath: string) =>
    call<StatusReport>("start_singbox", { configPath }),
  startManaged: (input: { manualOutbounds: Outbound[]; subscriptionLinks?: SubscriptionLinkRef[]; selectAllSubscriptionLinks: boolean; profile?: { subscription_id: string; child_key: string }; settings: GeneratorSettings }) =>
    call<ManagedLaunchResult>("start_managed_singbox", { input }),
  stop: () => call<StatusReport>("stop_singbox"),
  getStatus: () => call<StatusReport>("get_status"),
  getLogs: (limit = 500) => call<LogLine[]>("get_logs", { limit }),
  isRunning: () => call<boolean>("is_running"),
  getCurrentConfig: () => call<string | null>("get_current_config"),
  writeDefaultConfig: () => call<string>("write_default_config"),
  resetState: () => call<StatusReport>("reset_state"),

  parseLink: (link: string) => call<Outbound>("parse_link", { link }),
  parseLinks: (text: string) => call<ParseLinksResult>("parse_links", { text }),
  /** Auto-detect share-links vs subscription URLs in a mixed blob. */
  parseInput: (text: string) => call<ParsedInput>("parse_input", { text }),
  outboundToSingboxJson: (outbound: Outbound) =>
    call<Record<string, unknown>>("outbound_to_singbox_json", { outbound }),

  generateConfig: (outbounds: Outbound[], settings: GeneratorSettings) =>
    call<Record<string, unknown>>("generate_config", { outbounds, settings }),
  saveConfigToPath: (content: Record<string, unknown>, path?: string) =>
    call<string>("save_config_to_path", { content, path }),
  checkConfigWithBinary: (content: Record<string, unknown>) =>
    call<string>("check_config_with_binary", { content }),

  // Start sing-box with a known controller URL (used by
  // ConfigBuilder so the Clash API surface is reachable for
  // proxy switching).
  startSingboxWithConfig: (configPath: string, controllerUrl: string) =>
    call<StatusReport>("start_singbox_with_config", { configPath, controllerUrl }),

  listProxies: () => call<ProxiesResponse>("list_proxies"),
  selectProxy: (group: string, member: string) =>
    call<void>("select_proxy", { group, member }),
  testDelay: (name: string, timeoutMs?: number) =>
    call<number | null>("test_delay", { name, timeoutMs }),

  // Direct TCP ping (independent of sing-box). Works while the
  // tunnel is down so the user can see the best server before
  // connecting. Returns `null` on timeout / connection refused.
  pingEndpoint: (host: string, port: number, timeoutMs?: number) =>
    call<number | null>("ping_endpoint", { host, port, timeoutMs }),

  // Batch IP → ISO country-code lookup via ip-api.com. The result
  // is small and stable, so the frontend caches it in localStorage
  // and only re-asks for IPs it hasn't seen.
  lookupGeoip: (ips: string[]) =>
    call<[string, string][]>("lookup_geoip", { ips }),

  listSubscriptions: () => call<SubscriptionSnapshot>("list_subscriptions"),
  getReadyProfileMetadata: (subscriptionId: string, childKey: string) =>
    call<HomeProfileMetadata>("get_ready_profile_metadata", {
      input: { subscription_id: subscriptionId, child_key: childKey },
    }),

  addSubscription: (input: AddSubscriptionInput) => call<RefreshSubscriptionResult>("add_subscription", { input }),
  removeSubscription: (id: string) => call<void>("remove_subscription", { id }),
  refreshSubscription: (id: string) => call<RefreshSubscriptionResult>("refresh_subscription", { id }),
  setSubscriptionInterval: (id: string, intervalMinutes: number) => call<SubscriptionSummary>("set_subscription_interval", { id, intervalMinutes }),
  selectSubscriptionChild: (id: string, childKey: string) => call<SubscriptionSummary>("select_subscription_child", { id, childKey }),
  migrateLegacySubscriptions: (inputs: Array<{ id: string; name: string; url: string; intervalMinutes: number }>) => call<SubscriptionSnapshot>("migrate_legacy_subscriptions", { inputs }),
  getSubscriptionHwid: () => call<boolean>("get_subscription_hwid"),
  resetSubscriptionHwid: () => call<void>("reset_subscription_hwid"),


  getAutostart: () => call<boolean>("get_autostart"),
  setAutostart: (enabled: boolean) =>
    call<boolean>("set_autostart", { enabled }),

  // System proxy (Windows): route HTTP/HTTPS through sing-box.
  applySystemProxy: (host: string, port: number) =>
    call<void>("apply_system_proxy", { host, port }),
  clearSystemProxy: () => call<void>("clear_system_proxy"),

  // Running processes — used by the routing process-name picker
  // so the user can click on an .exe instead of typing its name.
  // Returns an empty array outside the Tauri shell (vite dev preview).
  listProcesses: () => call<ProcessInfo[]>("list_processes"),

  checkAppUpdate: () =>
    call<{
      version: string;
      current_version: string;
      available: boolean;
      notes: string;
    }>("check_app_update"),
  installAppUpdate: (expectedVersion?: string) =>
    call<void>("install_app_update", { expectedVersion }),

  // sing-box auto-update (separate from the app-shell updater).
  checkSingboxUpdate: () =>
    call<{
      current_version: string;
      latest_version: string;
      available: boolean;
      asset_name: string | null;
      size_bytes: number;
    }>("check_singbox_update"),
  applySingboxUpdate: (expectedVersion?: string) =>
    call<string>("apply_singbox_update", { expectedVersion }),
};

export { TauriCommandError };
