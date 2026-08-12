# v0.1.0 — Initial Release

First public release of **Singbox Client** — a cross-platform GUI VPN client built on
[Tauri 2](https://tauri.app/) and the [sing-box](https://sing-box.sagernet.org/) core.

> Source code: <https://github.com/markwhite7881-cpu/VPN>

---

## ✨ Features

### Subscriptions & profiles
- Import subscription via raw URL, base64, or Clash YAML
- Multi-profile manager (add / rename / delete / activate)
- Per-profile config JSON preview before applying

### Protocol parsers (clipboard links → sing-box outbounds)
- ✅ **VLESS** (with `reality`, `xtls-rprx-vision`, `flow`)
- ✅ **VMess** (legacy + AEAD)
- ✅ **Trojan**
- ✅ **Shadowsocks** (all modern ciphers; SIP002 URI)
- ✅ **Hysteria 2**
- ✅ **TUIC** v5

### Live config builder
- Tabs for: inbounds, outbounds, route, DNS, experimental
- Rule-set presets pulled from SagerNet (geosite-cn, geoip-cn, geoip-ru, ads)
- Atomic apply: validates JSON, writes to disk, (re)starts sing-box

### Traffic & routing
- WebSocket stream to sing-box's Clash API (`/traffic`)
- Per-server latency probe (TCP, async)
- GeoIP lookup for the active outbound (offline DB)

### Connection modes
- **system_proxy** — sets Windows registry to 127.0.0.1:2080 (via `winreg`)
- **tun** — embedded tun2socks (requires admin)

---

## 📦 What's in this archive

```
singbox-client-portable-v0.1.0.zip
├── Singbox Client.exe            # Tauri app, statically linked
├── sing-box-x86_64-pc-windows-msvc.exe   # sidecar v1.14.x
├── libcronet.dll                 # bundled by Tauri
└── README.txt                    # quick-start
```

> **Note:** the app embeds WebView2 via Tauri's `Webview2Loader.dll` (resolved at
> runtime from system). Windows 10 21H2+ / Windows 11 already have it. For older
> Windows installs, install the [Evergreen runtime](https://developer.microsoft.com/microsoft-edge/webview2/).

---

## 🚀 Quick start

1. Extract the archive anywhere (e.g. `C:\Apps\singbox-client\`).
2. Launch `Singbox Client.exe`.
3. **Subscriptions** → paste your subscription URL → **Fetch**.
4. **Servers** → pick a server → **Connect**.
5. To proxy all system traffic, switch connection mode to **system_proxy** in the
   Home tab. (Internet Settings will be set automatically; restored on disconnect.)

---

## 🛠️ Known issues / caveats

- **Rule-sets are downloaded on first launch** from `raw.githubusercontent.com`.
  No proxy is used for that fetch (chicken-and-egg). If you are offline, you can
  pre-place the four `.srs` files in the app's data dir.
- **system_proxy mode** edits `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings`
  on Windows. It does not touch VPN-interface settings and does not play nicely
  with apps that pin their own proxy.
- **TUN mode** is Windows-only in this build and requires admin elevation.
- This is `0.1.0` — there are rough edges. Please file issues at
  <https://github.com/markwhite7881-cpu/VPN/issues>.

---

## 🧱 Build info

- Tauri 2.x, Rust 1.96, Node 22, Vite 5
- sing-box sidecar: 1.14.x (`x86_64-pc-windows-msvc`)
- LTO + `opt-level = "s"` + `strip = true`
- Target: Windows 10 21H2+ / Windows 11 x64

---

SHA-256 checksums are listed in `SHA256SUMS.txt` next to the binary.
