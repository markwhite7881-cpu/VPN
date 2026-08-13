# v1.0.1 — Cloakwire rebrand

This is the first release under the **Cloakwire** name. v1.0.0 is the last release that shipped as "Singbox Client".

## What's new
- **New app icon** — the Cloakwire logo (a hooded cloak with a wifi signal inside) is now the icon everywhere: window, taskbar, installer, MSI/NSIS shortcuts. The logo was supplied by the user; we removed the black background with `rembg` so the icon blends with any Windows theme.
- **Brand rename** — product name, window title, header, and all user-facing strings now say "Cloakwire". The Rust package is now `cloakwire` and the binary is now `cloakwire.exe`.
- **Tauri auto-updater is live** — existing v1.0.0 installs will see v1.0.1 in the Home tab and can install + restart with one click. Manifest URL: `https://github.com/markwhite7881-cpu/cloakwire/releases/latest/download/latest.json`.
- **sing-box core auto-updater is live** — v1.0.1+ can pull newer sing-box binaries from GitHub without re-installing the app. The new binary lands in `%LOCALAPPDATA%\ru.classquiz.singbox\singbox-runtime\sing-box.exe` and is picked up by `ProcessManager::locate_binary` on the next start.

## Why 1.0.1, not 2.0
- The Tauri `identifier` (`ru.classquiz.singbox`) is preserved, so Windows still recognises v1.0.1 as an in-place upgrade of v1.0.0 — no uninstall / reinstall.
- No behaviour change, no config migration. Settings, subscriptions, manual profiles and routing rules are all carried over.
- The only user-visible change is the brand and the icon.

## Upgrading from v1.0.0
- **MSI**: install `Cloakwire_1.0.1_x64_en-US.msi` over the existing v1.0.0 install (Windows will offer "upgrade" because the ProductCode matches).
- **NSIS**: run `Cloakwire_1.0.1_x64-setup.exe` (NSIS does not auto-detect upgrades across product names — uninstall v1.0.0 first, or just use the auto-updater from inside the running v1.0.0).
- **Portable**: replace `Singbox-Client-1.0.0-portable.exe` with `Cloakwire-1.0.1-portable.exe`.

## Artifacts
- `Cloakwire_1.0.1_x64_en-US.msi` (27 MB) — Windows MSI installer, signed
- `Cloakwire_1.0.1_x64-setup.exe` (16 MB) — Windows NSIS installer, signed
- `Cloakwire-1.0.1-portable.exe` (7.4 MB) — portable single-file binary, signed
- `*.sig` sidecars (Ed25519 minisign signatures) for the updater
- `latest.json` — Tauri updater manifest

## Verification
- `cargo test --lib`: 56/56 pass
- `npm run build`: tsc + vite, 0 errors
- `tauri build`: MSI + NSIS produced cleanly
- Background-removed icon: alpha range 0-255, ~70% of pixels fully transparent
