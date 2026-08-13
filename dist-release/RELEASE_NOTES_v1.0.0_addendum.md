> **Note**: v1.0.0 was the last release under the "Singbox Client" name. The project has been renamed to **Cloakwire** as of v1.0.1 — see https://github.com/markwhite7881-cpu/cloakwire/releases/tag/v1.0.1. The v1.0.0 → v1.0.1 transition is an in-place upgrade (same `identifier`); no uninstall / reinstall needed. The GitHub repo was renamed from `VPN` to `cloakwire` shortly after v1.0.1 shipped; old `VPN` URLs still redirect.

## v1.0.0 - first stable release

### Highlights
- **Apps via VPN / Apps direct**: simple process-picker UI - pick .exe names, done
- **Routing 2.0**: full per-rule editor, drag-and-drop, all matchers, custom rule-sets, Loyalsoldier + meta-rules-dat presets
- **manual profile persistence** + immediate subscription refresh on launch
- **Latency pinger** restored (was accidentally disabled in v0.3.0)
- **Tauri auto-updater** for the app shell (in v1.0.1+)
- **sing-box auto-update** from GitHub releases (in v1.0.1+)
- DNS switched to Yandex 77.88.8.8 + DoH fallback 8.8.8.8
- TUN OS-level DNS fix (was hanging on the auto-derived /30 neighbour)

### Artifacts
- `Singbox.Client_1.0.0_x64_en-US.msi` (27 MB) - Windows MSI installer
- `Singbox.Client_1.0.0_x64-setup.exe` (16 MB) - Windows NSIS installer
- `Singbox-Client-1.0.0-portable.exe` (7.4 MB) - portable single-file
- `*.sig` sidecars (Ed25519)
- `latest.json` - Tauri updater manifest (pointed at v1.0.0; v1.0.1 supersedes)

### Verification
- `cargo test --lib`: 56/56 pass
- `npm run build`: tsc + vite, 0 errors
- `tauri build`: MSI + NSIS produced cleanly
