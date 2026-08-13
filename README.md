# Cloakwire

> A privacy-first VPN client for Windows, powered by sing-box.

Route traffic your way: send only selected apps through a VPN tunnel, or protect everything and exclude the apps that must stay direct.

[Download for Windows](../../releases) · [Report a bug](../../issues)

## Why Cloakwire?

Traditional VPN clients treat your entire system as one connection. Cloakwire gives you per-app control without writing routing rules or editing JSON configuration files.

- **Selected apps through VPN** — route only the browser, game, messenger, or other apps you choose.
- **Exclude apps from VPN** — protect all traffic while keeping selected apps on a direct connection.
- **Powered by sing-box** — use modern VPN and proxy protocols through a proven open-source core.
- **Simple by default** — subscriptions, profiles, and connection management in one interface.
- **Open source** — inspect, build, and improve the client yourself.

## How tunneling works

Cloakwire creates a protected route exactly where you need it.

### Route selected apps through VPN

Choose the apps that should use the VPN tunnel. Their connections are encrypted and sent through your selected profile, while all other traffic continues to use the direct connection.

```text
Browser / messenger / game ──> VPN tunnel ──> Internet
Other apps                  ──> Direct ──────> Internet
```

### Exclude selected apps from VPN

You can reverse the rule: send all system traffic through VPN and exclude only the apps that must stay direct — for example, banking software, corporate services, or local-network tools.

```text
All apps                    ──> VPN tunnel ──> Internet
Excluded apps               ──> Direct ──────> Internet
```

No manual routing rules. No hand-edited configuration files. Choose a mode, select the apps, and connect.

**You control the route — not the network configuration.**

## Supported protocols

Cloakwire is powered by [sing-box](https://github.com/SagerNet/sing-box). Protocol availability depends on the bundled sing-box version and the features exposed by the application interface.

The core supports modern proxy and VPN protocols, including VLESS, VMess, Trojan, Shadowsocks, Hysteria2, TUIC, and WireGuard.

## Quick start

1. Download the latest build from [Releases](../../releases).
2. Add a subscription URL or import a supported profile.
3. Choose **TUN mode** or **System Proxy** mode.
4. Select which apps use the tunnel — or which apps to exclude.
5. Connect.

## Privacy and security

Cloakwire is a client, not a VPN provider. It does not operate VPN servers or make your traffic anonymous by itself: your privacy also depends on the server, profile, and provider you choose.

The application delegates proxy and VPN protocol handling to sing-box rather than implementing cryptography itself. Keep your profiles and subscription links private, and download builds only from this repository's official releases.

## Development

Cloakwire is built with:

- [Tauri 2](https://v2.tauri.app/)
- [React](https://react.dev/)
- [TypeScript](https://www.typescriptlang.org/)
- [Rust](https://www.rust-lang.org/)
- [sing-box](https://github.com/SagerNet/sing-box)

## Contributing

Issues, feature requests, and pull requests are welcome. Please open an issue before beginning large changes so the implementation can be discussed first.

## License

Distributed under the [MIT License](LICENSE).