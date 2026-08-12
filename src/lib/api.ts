/**
 * Typed wrappers around the Tauri command surface.
 *
 * The Rust side serialises AppError as `{ kind, message }`. We surface
 * that as a JS Error so `try/catch` still works in the React code.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  BinaryInfo,
  GeneratorSettings,
  LogLine,
  Outbound,
  ParseLinksResult,
  ParsedInput,
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

  start: (configPath: string) =>
    call<StatusReport>("start_singbox", { configPath }),
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

  fetchSubscription: (url: string) =>
    call<ParseLinksResult>("fetch_subscription", { url }),

  getAutostart: () => call<boolean>("get_autostart"),
  setAutostart: (enabled: boolean) =>
    call<boolean>("set_autostart", { enabled }),

  // System proxy (Windows): route HTTP/HTTPS through sing-box.
  applySystemProxy: (host: string, port: number) =>
    call<void>("apply_system_proxy", { host, port }),
  clearSystemProxy: () => call<void>("clear_system_proxy"),
};

export { TauriCommandError };
