// Settings persistence for the mobile UI.
//
// The GeneratorSettings blob shares the desktop localStorage key
// (`singbox-client.settings.v2`) and merge strategy so both shells
// see the same routing/DNS/port choices. The v1 → v2 migration and
// the `final_outbound` fix-up live in the desktop App.tsx and are
// not repeated here — on Android there is no v1 key to migrate.
//
// `autoConnect` is a mobile-shell-only preference (the desktop
// starts/stops sing-box manually), stored under its own key.

import { DEFAULT_SETTINGS } from "@/lib/defaults";
import type { GeneratorSettings } from "@/lib/types";

const SETTINGS_KEY = "singbox-client.settings.v2";
const AUTO_CONNECT_KEY = "singbox.mobile.autoConnect";

export function loadSettings(): GeneratorSettings {
  if (typeof window === "undefined") return DEFAULT_SETTINGS;
  try {
    const raw = window.localStorage.getItem(SETTINGS_KEY);
    if (!raw) return DEFAULT_SETTINGS;
    const parsed = JSON.parse(raw);
    return {
      ...DEFAULT_SETTINGS,
      ...parsed,
      routing: { ...DEFAULT_SETTINGS.routing, ...(parsed.routing ?? {}) },
      clash_api: { ...DEFAULT_SETTINGS.clash_api, ...(parsed.clash_api ?? {}) },
    };
  } catch {
    return DEFAULT_SETTINGS;
  }
}

export function saveSettings(s: GeneratorSettings): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(SETTINGS_KEY, JSON.stringify(s));
  } catch {
    /* quota exceeded or storage disabled — non-fatal */
  }
}

export function loadAutoConnect(): boolean {
  if (typeof window === "undefined") return false;
  try {
    return window.localStorage.getItem(AUTO_CONNECT_KEY) === "1";
  } catch {
    return false;
  }
}

export function saveAutoConnect(v: boolean): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(AUTO_CONNECT_KEY, v ? "1" : "0");
  } catch {
    /* ignore */
  }
}
