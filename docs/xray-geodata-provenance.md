# Xray geodata provenance

Cloakwire provisions `geoip.dat` and `geosite.dat` only when the Xray fallback needs them.

- **Upstream:** [`Loyalsoldier/v2ray-rules-dat`](https://github.com/Loyalsoldier/v2ray-rules-dat)
- **Transport:** HTTPS GitHub Releases, using the latest release published by the upstream repository.
- **Integrity:** each data file is downloaded together with its upstream `.sha256sum` file; the exact filename is parsed and the SHA-256 digest is verified before installation.
- **Storage:** verified files are kept under the per-user Tauri application-data directory in `xray-geodata`. They are not bundled into the application, placed beside `xray.exe`, or committed to this repository.
- **Refresh policy:** a verified pair is reused for 24 hours. A stale pair remains usable if a refresh fails; initial provisioning fails with a sanitized engine-unavailable error.
- **Update behavior:** the complete pair and state record are replaced only after both files pass verification. Partial or invalid downloads never become the active pair.

The upstream project supplies the routing database files used by Xray. Distribution and use remain subject to the upstream repository's license and attribution requirements; users should consult that repository for the current license text and notices.
