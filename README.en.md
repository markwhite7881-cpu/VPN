<div align="center">

<img src="src-tauri/icons/icon.png" alt="Cloakwire" width="128" />

# Cloakwire

**Privacy-first desktop VPN client with sing-box as its primary core and Xray as a capability fallback.**

Tauri 2 + React + TypeScript.

[![Release](https://img.shields.io/github/v/release/markwhite7881-cpu/cloakwire?include_prereleases&sort=semver&style=for-the-badge)](https://github.com/markwhite7881-cpu/cloakwire/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/markwhite7881-cpu/cloakwire/total?style=for-the-badge)](https://github.com/markwhite7881-cpu/cloakwire/releases/latest)
[![License](https://img.shields.io/github/license/markwhite7881-cpu/cloakwire?style=for-the-badge)](LICENSE)
[![Stars](https://img.shields.io/github/stars/markwhite7881-cpu/cloakwire?style=for-the-badge)](https://github.com/markwhite7881-cpu/cloakwire/stargazers)

</div>

> 🇷🇺 **Russian version:** [README.md](README.md)

---

## 🎯 What is this

**Cloakwire** is a minimal desktop VPN client for Windows, macOS, and Linux. It accepts share links and subscriptions, stores configuration locally, and starts a profile without requiring users to hand-edit JSON.

**sing-box is the primary core.** It handles ordinary supported profiles, TUN, System Proxy, per-app routing, proxy selection, and built-in latency checks.

**Xray is a capability fallback.** When a subscription contains a profile that is safer or more correct to run through Xray, Cloakwire prepares and launches it automatically. Home still shows the connection state, selected server, and live traffic. Proxy-group controls and built-in delay tests are available only while sing-box is active; this is an intentional boundary, not a UI defect.

### Control the route, not network settings

**Apps via VPN.** Select a browser, game, messenger, or another application — only its connections use the VPN tunnel. The rest of the traffic remains direct.

**Apps direct.** Or invert the policy: route system traffic through the VPN while keeping selected apps direct, such as banking clients, corporate services, or local-network tools.

---

## ✨ Features

| | |
|---|---|
| 🚀 **Fast start** | Share link or subscription → a profile ready to connect |
| 🧩 **Two VPN cores** | sing-box is primary; Xray is an automatic fallback for compatible profiles |
| 🎯 **Per-app routing** | “Telegram via VPN, bank app direct” in one interface |
| 🗂️ **Private subscriptions** | Subscriptions are parsed in the backend; provider URLs and profile bodies are not exposed to the WebView |
| 🧭 **Clear Home screen** | Servers from one subscription are grouped; provider titles are used as a fallback name |
| 🔄 **Safe reconnects** | A running VPN reconnects after server changes and applicable Config/Routing changes |
| 📈 **Live status** | Connection state, traffic, and the active runtime engine |
| 🔓 **Open source** | MIT, no analytics or telemetry |

Supported link protocols include VLESS, VMess, Trojan, Shadowsocks, Hysteria2, and TUIC. Compatibility of a particular profile depends on its parameters and selected runtime engine.

---

## 📥 Install

Download files from **[Releases → Latest](https://github.com/markwhite7881-cpu/cloakwire/releases/latest)**. The list below is the payload of the current `v1.3.1` release.

### Windows x64

- **`Cloakwire_1.3.1_x64-setup.exe`** — NSIS installer.

> ⚠️ The `v1.3.1` installer is not Authenticode-signed. Windows SmartScreen may warn about an unknown publisher. Download only from GitHub Releases and compare its SHA-256 with `SHA256SUMS.txt`.

### macOS

Choose the build that matches your processor:

| Mac | Files |
|---|---|
| Intel | `Cloakwire_1.3.1_x64.dmg` or `Cloakwire_1.3.1_x64.app.zip` |
| Apple Silicon | `Cloakwire_1.3.1_aarch64.dmg` or `Cloakwire_1.3.1_aarch64.app.zip` |

> ⚠️ The `v1.3.1` macOS builds are unsigned and not notarized. macOS may require an explicit launch approval in Privacy & Security.

### Linux x64 — Ubuntu / Debian

Install the package:

```bash
sudo apt install ./Cloakwire_1.3.1_amd64.deb
cloakwire
```

The package installs `/usr/bin/cloakwire`, `sing-box`, and Xray. Its `postinst` grants sing-box `cap_net_admin,cap_net_raw=+ep`, required for the supported Linux TUN path:

```bash
getcap /usr/bin/sing-box
# expected: /usr/bin/sing-box cap_net_admin,cap_net_raw=ep
```

If an update or manual permission change stripped the capability, restore it:

```bash
sudo setcap cap_net_admin,cap_net_raw=+ep /usr/bin/sing-box
```

The Linux build targets Ubuntu 22.04+ and Debian 12+ desktops. TUN mode is recommended because it captures traffic at the network layer rather than relying on each application’s proxy support.

### Verify a download

Every release contains `SHA256SUMS.txt`. For example, on Windows:

```powershell
Get-FileHash .\Cloakwire_1.3.1_x64-setup.exe -Algorithm SHA256
```

Compare the result with the corresponding entry in `SHA256SUMS.txt`.

---

## 🚀 First run

1. Launch Cloakwire. TUN mode may require administrator privileges.
2. In **Servers**, paste a share link (`vless://…`) or subscription URL, then select **Add**.
3. In **Routing**, add applications to **Apps via VPN** or **Apps direct**.
4. In **Config**, choose a tunnel mode — usually **TUN**.
5. On **Home**, select a server and press the power button.

When the VPN is already running, selecting another server or changing applicable settings triggers a safe reconnect. The UI keeps the notice visible while reconnecting or when a retry is needed.

---

## 🖼️ Interface

### Home — main screen

Subscription server groups, selected profile, active-engine status, live Download/Upload, and pre-connect server latency.

![Home tab](dist-release/screenshots/01-home.png)

### Servers — profiles and subscriptions

Supports share links plus URL, base64, and Clash YAML subscriptions. Formats are detected automatically; unnamed subscriptions can use provider metadata as their display name.

![Servers tab](dist-release/screenshots/02-servers.png)

### Config — tunnel modes and DNS

Four modes: **TUN**, **System Proxy**, **Both**, and **None**. Local and remote DNS can be configured independently.

![Config tab](dist-release/screenshots/03-config.png)

### Routing — simple and advanced modes

The simple mode provides **Apps via VPN** and **Apps direct**. Advanced mode adds custom rules, rule sets, sniffing, automatic interface detection, and a final outbound.

![Routing tab — simple UX](dist-release/screenshots/04-routing.png)

![Routing tab — Advanced](dist-release/screenshots/05-routing-advanced.png)

---

## 🏗️ Architecture

```text
┌──────────────────────┐
│ React + Tailwind     │  ← typed tauri::invoke
│ src/                 │
└──────────┬───────────┘
           │
┌──────────▼───────────┐
│ Rust + Tauri 2       │  ← subscriptions, routing, lifecycle, safe IPC
│ src-tauri/src/       │
└──────────┬───────────┘
           │ process lifecycle
┌──────────▼───────────────────────────┐
│ sing-box (primary)  │ Xray (fallback) │
│ TUN / proxy control │ compatible      │
│ Clash API / delay   │ profile runtime │
└──────────────────────────────────────┘
```

1. **Frontend** — React + TypeScript + Tailwind. It receives only the safe data required by the interface.
2. **Tauri shell** — Rust layer for subscriptions, runtime-config generation, process lifecycle, routing, DNS, and updates.
3. **VPN runtime** — sing-box is used by default; Xray launches classified fallback profiles. Only one engine runs at a time.

---

## 🛠️ Stack

| Layer | Technology |
|---|---|
| Shell | **Tauri 2** (Rust + WebView) |
| UI | **React 18** + **TypeScript 5** |
| Styling | **Tailwind CSS 3** + design tokens |
| Primary VPN core | [**sing-box**](https://github.com/SagerNet/sing-box) sidecar |
| Fallback VPN core | [**Xray-core**](https://github.com/XTLS/Xray-core) sidecar |
| Routing | sing-box rules / rule sets + process routing |
| App updates | Tauri updater infrastructure; signed updater artifacts are published separately when available |
| Runtime updates | Managed sing-box updates exist; automatic Xray updates are intentionally not enabled |

---

## 🔐 Security and data boundaries

- Subscription URLs, profile bodies, UUIDs/keys, and runtime paths are not exposed to the WebView.
- Xray runtime configuration and its stdout/stderr stay in the backend.
- When Xray is active, the UI does not call the sing-box Clash API for proxy lists, proxy selection, or delay tests.
- No product telemetry or analytics.
- Verify downloadable artifacts with SHA-256; use the release’s `SHA256SUMS.txt`.

---

## 🧑‍💻 Build from source

```powershell
# Requirements: Node 20+, stable Rust, and Tauri dependencies for the target platform
git clone https://github.com/markwhite7881-cpu/cloakwire.git
cd cloakwire
npm ci
npm run tauri:build
```

To build the Linux `.deb`, use Ubuntu 22.04+ / Debian 12+ or WSL2 and run:

```bash
./scripts/build-linux-deb.sh 1.3.1
```

The resulting package’s `postinst` grants the TUN capability only to `sing-box`, because it owns the supported Linux TUN path.

---

## 🤝 Contributing

PRs are welcome. For larger changes, please open an issue first to discuss direction.

- **Code style:** `cargo fmt` for Rust, Prettier for TS/TSX.
- **Checks:** `npm test`, then a production build; inspect artifacts before publication.
- **Commits:** conventional commits (`feat:`, `fix:`, `docs:`, `chore:`).

---

## 📜 License

[MIT](LICENSE) — do what you want, just credit the authors.

---

## 🙏 Thanks

- [**SagerNet/sing-box**](https://github.com/SagerNet/sing-box)
- [**XTLS/Xray-core**](https://github.com/XTLS/Xray-core)
- [**Tauri**](https://tauri.app)

---

<div align="center">

**[⬇ Download latest](https://github.com/markwhite7881-cpu/cloakwire/releases/latest)** · **[🐛 Report a bug](https://github.com/markwhite7881-cpu/cloakwire/issues)** · **[⭐ Star this repo](https://github.com/markwhite7881-cpu/cloakwire)**

Made with care for people who value their privacy.

</div>
