# v1.0.2 — header logo + TUN-gated routing

## What's new

- **App header now uses the real Cloakwire logo** — the hooded-cloak-with-wifi mark with transparent background replaces the generic `ShieldCheck` lucide icon in the top-left of every screen. Served from `src/assets/cloakwire-logo.png` (a copy of the bundled `icon.png`).

- **Routing tab is now read-only when not in TUN mode** — process-based rules (`process_name` / `process_path`, the whole point of the simple-UX pickers) only fire when sing-box intercepts traffic at the OS level. In `system_proxy` mode the proxy doesn't see the originating process, so those matchers silently never match. To prevent the user from configuring rules that look correct but do nothing, the entire Routing tab is now gated:
  - Amber banner at the top of the tab explains the requirement, names the current mode (`system_proxy` / `none`), and points at the Config tab.
  - Both process-picker cards: chip remove buttons are disabled with a "Switch to TUN mode to edit" tooltip; the cards themselves get `opacity-70` + `aria-disabled`.
  - `ProcessPicker` (the inline "Pick from running processes…" panel): toggle button, search input, and process rows are all disabled via a new `disabled` prop.
  - Advanced section: each sub-panel (general settings, custom rules, rule-sets, preset library) is wrapped in `pointer-events-none opacity-60` so every inner control (checkbox, select, drag handle, edit/delete button) becomes unclickable without touching them individually. The summary gets a `READ-ONLY` badge on the right.
  - The Reset button in the tab header is also disabled.
  - The JSON preview at the bottom of Advanced stays interactive — useful for the user to see what their existing config looks like while they decide whether to switch to TUN.

  When the user switches `tunnel_mode` to `tun` or `both` on the Config tab and returns to Routing, the banner disappears and every control comes back to life automatically — no state to clear, no reload needed.

## What didn't change

- Storage layout, settings, subscriptions, manual profiles, routing rules — all carried over from v1.0.1. This is purely a UX/improvement release.
- Identifier `ru.classquiz.singbox` is still the same, so v1.0.1 → v1.0.2 is an in-place upgrade (MSI/NSIS recognise the existing install).
- The Tauri updater endpoint in `tauri.conf.json` was already updated to the new (`/cloakwire/...`) URL after the repo rename; v1.0.2 ships the same endpoint in the binary.

## Artifacts

- `Cloakwire_1.0.2_x64_en-US.msi` (27 MB) — Windows MSI installer, signed
- `Cloakwire_1.0.2_x64-setup.exe` (16 MB) — Windows NSIS installer, signed
- `Cloakwire-1.0.2-portable.exe` (7.4 MB) — portable single-file, signed
- `*.sig` sidecars (Ed25519 minisign signatures) for the updater
- `latest.json` — Tauri updater manifest

## Verification

- `cargo test --lib`: 56/56 pass
- `npm run build`: tsc + vite, 0 errors
- `tauri build`: MSI + NSIS produced cleanly
- Portable exe FileVersion/ProductVersion: `1.0.2` ✓
- Auto-updater chain: `…/VPN/releases/latest/download/latest.json` → 301 → `…/cloakwire/releases/latest/download/latest.json` → 301 → `…/cloakwire/releases/download/v1.0.2/latest.json` → serves `version: 1.0.2`. Old v1.0.1 installs follow the redirect and pick up the update.
