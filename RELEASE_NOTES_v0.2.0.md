# v0.2.0 — Routing 2.0

The big change in this release: **routing is no longer six boolean
checkboxes**. You can now define an ordered list of rules with
fine-grained matchers (domain, IP CIDR, port, network, protocol,
rule-set, process name, …), drag-to-reorder, and toggle each rule
individually. The Rust-side config generator passes the rules
through to sing-box verbatim.

> Source: <https://github.com/markwhite7881-cpu/VPN>
> Docs: `docs/ROUTING_PLAN.md` in the repo

---

## ✨ What's new

### Routing 2.0 (the headline feature)
- **New dedicated "Routing" tab** with sortable, expandable rules list
- **All sing-box 1.14+ matchers** as chips:
  domain, domain_suffix, domain_keyword, domain_regex, ip_cidr,
  source_ip_cidr, port, port_range, source_port, source_port_range,
  network, protocol, client, process_name, process_path, rule_set
- **All actions** as buttons: `route` (with outbound picker),
  `reject`, `hijack-dns`, `sniff`, `resolve`
- **Per-rule toggle** (enable/disable) and **invert** flag
- **Custom rule-sets** — add any Loyalsoldier / meta-rules-dat URL or
  any custom `.srs` / `.json` URL; toggle each one
- **Preset library** with starter rules and 11 pre-built rule-sets
  (Loyalsoldier geoip-cn/ru + geosite-ads, plus meta-rules-dat
  geosite-geolocation-cn, geosite-category-ads-all, geosite-malware,
  geosite-phishing, geosite-cryptominers, geosite-gfw, geosite-cn,
  geosite-geolocation-!cn, and private IP rule-set)
- **Toggle source** in the preset library to filter Loyalsoldier vs
  meta-rules-dat
- **Live JSON preview** — see the generated `route` block in real time
  and copy it to clipboard
- **"Missing rule-set" warning** — if a rule references a rule-set tag
  that isn't enabled, a yellow banner lists them so you don't ship a
  silently-broken config
- **Drag-to-reorder** via `@dnd-kit/sortable` (~30KB; full keyboard
  accessibility, no HTML5 drag-and-drop quirks)

### Silent migration v1 → v2
The localStorage key was bumped to `singbox-client.settings.v2`. If
you had v0.1.0 settings, your boolean flags are converted into
individual `CustomRule` entries on first launch and saved as v2. The
v1 key is then removed. **You don't have to do anything.**

The migration is lossless and intentional:
- `bypass_lan: true` → "Bypass LAN" rule with the full RFC1918 +
  IPv6 ULA list
- `reject_ipv6: true` → "Reject IPv6" rule
- `block_quic: true` → "Block QUIC" rule (UDP/443)
- `bypass_cn: true` → "Bypass CN" rule + auto-add `geoip-cn` rule-set
- `bypass_ru: true` → "Bypass RU" rule + auto-add `geoip-ru` rule-set
- `block_ads: true` → "Block Ads" rule + auto-add `geosite-ads` rule-set

### Backend (`src-tauri/src/config/mod.rs`)
- `RoutingOptions` rewritten as `{ rules, rule_sets, sniff,
  final_outbound, auto_detect_interface, default_domain_resolver }`
- `matchers` and `action` are passed through as JSON, so adding new
  sing-box fields in the UI doesn't require Rust changes
- Disabled rules / rule-sets are filtered at config-generation time
- Tests (`cargo test`) — 49 unit tests pass, including new
  `disabled_rules_are_skipped`

### Dependencies
- `@dnd-kit/core@^6.3.1` (~12KB)
- `@dnd-kit/sortable@^10.0.0` (~14KB)
- `@dnd-kit/utilities@^3.2.2` (~3KB)

Total bundle size impact: ~30KB raw, ~9KB gzipped.

---

## 🛠️ What's gone
- The 6 boolean routing checkboxes that used to live inside the
  Config tab. If you haven't migrated yet, see "Silent migration"
  above.
- `bypass_lan`, `reject_ipv6`, `block_quic`, `bypass_cn`,
  `bypass_ru`, `block_ads` are no longer top-level `GeneratorSettings`
  fields. They've been replaced by `routing.rules[]`.

---

## 📦 What's in this archive

```
singbox-client-v0.2.0-portable-windows-x64.zip
├── Singbox Client.exe            # Tauri app, statically linked
├── sing-box-x86_64-pc-windows-msvc.exe   # sidecar 1.14.0
├── libcronet.dll                 # wry transport
└── README.txt                    # quick-start
```

Plus MSI and NSIS installers in the Assets section.

---

## ⚠️ Heads up: rule-set URLs

We use **Loyalsoldier / v2ray-rules-dat** for the preset URLs in the
UI (matches what the broader Xray/V2Ray ecosystem has standardised on).
The Rust-side `examples/verify_user_link.rs` test, in contrast, uses
**SagerNet / sing-geosite** and **SagerNet / sing-geoip** because
those are the canonical sing-box 1.14+ sources and the v0.1.0 tests
were already pinning them.

Both sources produce the same `.srs` binary format, so they
interoperate. If you find a Loyalsoldier URL 404s, switch the source
toggle in the preset library to "meta" and pick the equivalent
`geosite-*` / `geoip-*` entry.

---

## 🧱 Build info
- Tauri 2.1, Rust 1.96, Node 22, Vite 5
- sing-box sidecar 1.14.0 (`x86_64-pc-windows-msvc`)
- LTO + `opt-level = "s"` + `strip = true`
- Target: Windows 10 21H2+ / Windows 11 x64

---

## 📝 Files of interest

| Path | What |
|---|---|
| `src/components/routing/RoutingTab.tsx` | Main container |
| `src/components/routing/RuleList.tsx` | Sortable list with `@dnd-kit` |
| `src/components/routing/RuleRow.tsx` | Compact row, expands to editor |
| `src/components/routing/RuleEditor.tsx` | Inline matcher + action editor |
| `src/components/routing/PresetPicker.tsx` | Starter rules + rule-set library |
| `src/components/routing/RuleSetsPanel.tsx` | Manage custom rule-sets |
| `src/lib/presets.ts` | Preset data (Loyalsoldier + meta-rules-dat) |
| `src/lib/types.ts` | `CustomRule`, `CustomRuleSet`, `RuleAction`, `RuleMatchers` |
| `src/App.tsx` | Tab + migration logic |
| `src-tauri/src/config/mod.rs` | Backend mirror (pass-through JSON) |
| `docs/ROUTING_PLAN.md` | Design notes for this change |

SHA-256 checksums are in `SHA256SUMS.txt` next to the binaries.
