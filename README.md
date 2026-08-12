# Singbox Client

Cross-platform GUI VPN client built on top of the [sing-box](https://github.com/SagerNet/sing-box) core.
Tauri 2.x + React + TypeScript, styled after [classquiz](https://classquiz.ru).

## Project status

| Stage | Status | Notes |
|-------|--------|-------|
| Этап 1. Tauri scaffold + sidecar | ✅ done | Spawn/stop, log streaming, health-check, default config |
| Этап 2. Protocol link parser | ✅ done | vless / vmess / trojan / ss / hy2 / tuic |
| Этап 3. Config generator | ✅ done | TUN inbound, route rules, selector/urltest groups, `sing-box check` clean |
| Этап 4. Clash API integration | ✅ done | list/select/test_delay over HTTP API |
| Этап 5. Server list + traffic chart | ✅ done | WebSocket traffic stream, SVG sparklines |
| Этап 6. Subscription auto-refresh | ✅ done | Fetch + parse + per-line errors + auto-refresh tick |
| Этап 7. Routing + autostart | ✅ done | Embedded geosite/geoip presets (ads/CN/RU/QUIC) + Windows autostart |
| Этап 8. Real-server testing | ⏳ next |  |

## Architecture

```
┌────────────────────┐
│  React + Tailwind  │  ← tauri::invoke for everything
│  (src/)            │
└────────┬───────────┘
         │ tauri::invoke  (typed commands)
┌────────▼───────────┐
│  Rust backend      │  ← process manager, state, logs
│  (src-tauri/src)   │
└────────┬───────────┘
         │ tokio::process::Command
┌────────▼───────────┐
│  sing-box sidecar  │  ← bundled as binaries/sing-box-<triple>.exe
│  + libcronet.dll   │
└────────────────────┘
```

The Rust process never reimplements any protocol. The whole networking stack
is delegated to sing-box; we just generate `config.json` and monitor the process.

## Layout

```
singbox-client/
├─ src/                       Frontend (React + TS)
│  ├─ App.tsx                 main screen
│  ├─ components/             Button, Card, Badge, StatusPill, LogView,
│  │                          ProfileCard, ConfigBuilder, ProxiesCard,
│  │                          TrafficCard, SubscriptionsCard
│  ├─ hooks/                  useTrafficStream, useSubscriptions
│  ├─ lib/                    api.ts (Tauri wrappers), types.ts, utils.ts,
│  │                          previewConfig.ts (browser mirror of Rust)
│  └─ index.css               design tokens (HSL variables, classquiz palette)
├─ src-tauri/
│  ├─ Cargo.toml
│  ├─ tauri.conf.json
│  ├─ capabilities/default.json
│  ├─ icons/                  32/128/128@2x.png, multi-res .ico
│  ├─ binaries/               sing-box-<triple>.exe + libcronet.dll
│  └─ src/
│     ├─ main.rs              entry point
│     ├─ lib.rs               tauri::Builder, plugin registration
│     ├─ process.rs           sing-box lifecycle, log ring buffer, watcher
│     ├─ commands.rs          #[tauri::command] surface
│     ├─ clash_api.rs         proxy list / select / test_delay
│     ├─ traffic.rs           /traffic WebSocket stream
│     ├─ config/mod.rs        TunnelMode, RoutingOptions, config generator
│     ├─ parser/              vless / vmess / trojan / ss / hy2 / tuic / to_json
│     └─ error.rs             AppError + AppResult
├─ scripts/make_icons.py      regenerate icons (run on demand)
├─ vite.config.ts
├─ tailwind.config.js
├─ tsconfig.json
└─ package.json
```

## Develop

```powershell
# 1. install JS deps
npm install

# 2. run Tauri dev (compiles Rust + starts Vite + opens window)
npm run tauri:dev

# 3. build release installer
npm run tauri:build
```

`sing-box` is bundled inside `src-tauri/binaries/`. To upgrade the core:

1. Download the latest `sing-box-<ver>-windows-amd64.zip` from
   <https://github.com/SagerNet/sing-box/releases>.
2. Extract `sing-box.exe` and `libcronet.dll` into `src-tauri/binaries/`.
3. Rename `sing-box.exe` to `sing-box-x86_64-pc-windows-msvc.exe`.

## Stage 1 smoke test

1. `npm run tauri:dev`
2. Click **Use default** in the Config card — the backend writes a
   minimal config to `%TEMP%` and validates it with `sing-box check`.
3. Click **Connect**. The hero card flips to "Tunnel is up", the
   process card shows PID + uptime, and logs start streaming.
4. `curl -x socks5h://127.0.0.1:2080 https://api.ipify.org` should
   return your real IP (the default config uses a `direct` outbound).
5. Click **Disconnect** to stop sing-box.

## Stage 3 — verify the generated config against the real sidecar

Two examples live in `src-tauri/examples/`:

```powershell
Set-Location C:\Users\Алексей\.minimax-agent\projects\singbox-client\src-tauri
$env:PATH = "C:\Users\Алексей\.cargo\bin;$env:PATH"

# Skinny: uses only SS/VMess/TUIC (no X25519 keys required).
# Passes `sing-box check` end-to-end against the bundled binary.
cargo run --example verify_config_skinny

# Full: VLESS+Reality, Hy2, SS, Trojan. Passes `sing-box check` of
# the structure; the X25519 public key is a placeholder so the
# actual `check` step warns about it.
cargo run --example verify_config
```

Both write the generated JSON to `src-tauri/examples/verify_output*.json`
and then run `sing-box check -c <file>`. The skinny version passes with
exit 0; the full version explains the placeholder credentials.

If you want to feed the result into a real sing-box:

```powershell
& .\src-tauri\binaries\sing-box-x86_64-pc-windows-msvc.exe run `
  -c .\src-tauri\examples\verify_output_skinny.json
```

## Stage 6 — subscription auto-refresh

The **Subscriptions** card lets you paste one or more subscription URLs.
Each URL is fetched on demand; the response (plain or base64) is split
per line, every line is parsed as a share-link, and the resulting
profiles are merged into the main list at the top of the screen.

```powershell
# The backend command:
tauri::invoke('fetch_subscription', { url: 'https://...' })
# → { lines: 12, ok: 11, failed: [{ line, error }] }
```

State is persisted to `localStorage` so URLs survive a refresh; the
hook re-fetches every 30 s only when at least one subscription is
configured. A failed line shows the parser error next to its index so
you can see exactly which line was rejected.

## Stage 7 — routing presets + Windows autostart

The **Config builder** card exposes six routing toggles in addition to
the two base ones (bypass-LAN, reject-IPv6):

| Toggle | sing-box rule(s) |
|--------|------------------|
| Block QUIC | `port_range: ["443:443"], network: udp, action: reject` |
| Block ads | `geosite: category-ads-all, action: reject` |
| Bypass CN | `geosite: [cn], action: direct` + `geoip: [cn], action: direct` |
| Bypass RU | `geoip: [ru], action: direct` |

All presets use sing-box's **embedded** classifier — no `.dat` file
download required. The order of `route.rules` is fixed (sniff → reject
→ direct → final) so the more specific rules always win.

The **autostart** toggle is a thin wrapper around
`tauri-plugin-autostart`. When enabled it writes a value under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`; when disabled it
deletes it. The plugin is configured to forward `--minimized` so an
autostart-on-login can choose to start hidden.
