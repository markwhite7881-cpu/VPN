# Routing 2.0 — design plan

> Status: research draft, awaiting scope approval.
> Author: mavis session `mvs_20e0b23453f540e6bf2dbaf60554aa7c` (2026-08-12)
> Not committed yet — research artifact only.

---

## 1. TL;DR

Текущий routing живёт в `ConfigBuilder.tsx` как шесть boolean-чекбоксов и в
`previewConfig.ts` как жёстко зашитый массив `rules`. Этого хватает на
"включил-выключил пресет", но не на реальное управление трафиком.

Цель — отдельный **Routing tab** с упорядоченным списком правил, drag-reorder,
редактором каждого правила, rule-set picker'ом и **кастомными правилами**
(domain / IP / port / process / rule_set). Бэкенд (`src-tauri/src/config/`) и
preview (`src/components/previewConfig.ts`) должны зеркалить новый формат.

---

## 2. Текущее состояние (recon)

| Где | Что |
|---|---|
| `src/lib/types.ts:165-173` | `RoutingOptions` — 7 boolean-флагов + `final_outbound: string` |
| `src/components/ConfigBuilder.tsx:31-55` | `DEFAULT_SETTINGS.routing` — те же флаги |
| `src/components/previewConfig.ts:88-188` | Генератор `route.rules` и `route.rule_set` в фиксированном порядке: `sniff → reject_ipv6 → reject_quic → bypass_lan (cidr) → rule_sets` |
| `screenshots/07-stage7-routing.png` | UI: 5 чекбоксов внутри Config builder. Никакого списка правил, никакого reorder |
| `App.tsx:56-143` | `GeneratorSettings` → `localStorage["singbox-client.settings.v1"]` (versioned key) |
| `src-tauri/src/config/mod.rs` | Rust-зеркало `GeneratorSettings` (источник правды для рантайма) |

**Чего нет:**
- Per-rule editor (только пресеты)
- Drag-to-reorder правил
- Кастомный domain / IP / port / process матчер
- Rule-set picker (rule-sets захардкожены в `previewConfig.ts`)
- Импорт правил (Clash YAML, sing-box JSON)
- Экспорт/шеринг пресета

---

## 3. Reference research (что я посмотрел)

### 3.0 Скоуп ресёрча (честный)

| Реф | Покрытие | Что взял |
|---|---|---|
| `SagerNet/sing-box` (testing) | ✅ Полный (route, rule, rule_action, rule-set) | Источник правды по конфигу |
| `HagerNet/sing-box` (rule-set types) | ✅ Полный | Inline/Local/Remote rule-set semantics |
| `HagerNet/sing-box` (headless-rule) | ✅ Полный | Список matcher'ов для inline rule-set |
| `HagerNet/sing-box` (route structure) | ✅ Полный | Top-level route fields (final, auto_detect_interface, ...) |
| `clash-verge-rev` (services/) | ✅ api.ts, cmds.ts, events.ts, delay.ts, i18n.ts, query-client.ts, update.ts, traffic-monitor-worker.ts | Чистое разделение слоёв (api vs cmds) |
| `clash-verge-rev` (pages/) | ✅ _layout, _navigation, _routers, _theme, connections, home, logs, profiles, proxies, **rules**, settings, unlock | **rules.tsx всего 3KB — read-only список**, наш шанс |
| `clash-verge-rev` (types/) | ✅ proxy-view.ts, global.d.ts, monaco.ts | Структура типов для rule/proxy |
| `clash-verge-rev` (components/rule/) | ✅ rule-item.tsx, provider-button.tsx | UI паттерн строки правила |
| `hiddify/hiddify-app` (lib/singbox/model/) | ✅ singbox_rule.dart, singbox_config_enum.dart, singbox_config_option.dart, singbox_outbound.dart, singbox_proxy_type.dart | Flat per-row rule design (domains как newline-string) |
| `Leadaxe/singbox-launcher` (Go, **не Tauri**) | ✅ `core/`, особенно `config_service.go` (23KB), `rebuild.go` (12KB), `template/` (presets), `warp/` (helper config) | Архитектурный паттерн config/template/rebuild, **не код** |
| `shaonhuang/vpn-link-serde` 0.1.5 (MIT, Rust) | ✅ README + docs.rs (полный API) | VLess/VMess/SS/Trojan/Hy2, **без TUIC**; regression oracle, не замена |
| `2dust/v2rayN` (C#) | ⚠️ Только root listing | Не копал глубже — C# WinForms, не наш стек |
| `MKultra6969/MK_XRAYchecker` (Python) | ⚠️ Только root listing | Proxy testing, не парсинг — не применимо |

### 3.1 sing-box официальная документация (`testing` branch, июль 2026)

Pulled raw markdown с `SagerNet/sing-box/testing/docs/configuration/`:

- **`route/index.md`** (4.6 KB) — top-level структура `route.{rules, rule_set, final, auto_detect_interface, ...}`. 1.14+ добавил `default_http_client`, `find_neighbor`, `dhcp_lease_files`.
- **`route/rule.md`** (11.4 KB) — все matcher'ы. Полный список полей:
  - Network: `inbound`, `ip_version`, `network` (tcp/udp/icmp), `auth_user`
  - Sniff: `protocol` (http/tls/quic/dns/...), `client` (since 1.10)
  - Domain: `domain`, `domain_suffix`, `domain_keyword`, `domain_regex`
  - IP: `ip_cidr`, `source_ip_cidr`, `ip_is_private`, `source_ip_is_private`
  - Port: `port`, `port_range`, `source_port`, `source_port_range`
  - Process: `process_name`, `process_path`, `process_path_regex` (1.10+)
  - Android: `package_name`, `package_name_regex` (1.14+)
  - Linux: `user`, `user_id`
  - Mobile: `network_type`, `network_is_expensive`, `network_is_constrained`, `wifi_ssid/bssid`
  - OS: `interface_address`, `default_interface_address`, `preferred_by`
  - 1.14+: `source_mac_address`, `source_hostname`
  - Reference: `rule_set` (array of tags), `rule_set_ip_cidr_match_source` (1.10+)
  - Logic: `invert`, `type: logical` + `mode: and|or`
  - Default rule merge: AND across categories, OR within.
- **`route/rule_action.md`** (8.5 KB) — actions:
  - Final: `route` (+ `outbound`, `route-options` block), `bypass` (1.13+), `reject` (+ `method: default|drop|reply`, `no_drop`), `hijack-dns`
  - Non-final: `route-options` (override_address, tls_fragment, tls_record_fragment, tls_spoof 1.14+), `sniff` (+ sniffer array, timeout), `resolve` (+ server, strategy, client_subnet, disable_cache)
- **`rule-set/index.md`** (3.4 KB) — три формы:
  - `type: inline` (since 1.10) — `{ tag, rules: [] }`
  - `type: local` — `{ tag, format: source|binary, path }` (auto-reload since 1.10)
  - `type: remote` — `{ tag, format, url, update_interval, http_client (1.14+), initial_path (1.14+), download_detour (deprecated 1.14) }`
- **`rule-set/headless-rule.md`** (6 KB) — то же что `rule.md` минус `action/outbound` (т.к. задаётся во внешнем rule, который ссылается на rule-set)
- **`rule-set/source-format.md`** (1.1 KB) — JSON `{ version: 1..5, rules: [] }`, компилируется в `.srs` через `sing-box rule-set compile`

**Ключевая новость:** в sing-box 1.12+ legacy `geosite`/`geoip` matcher'ы **удалены**, остались только `rule_set` ссылки. Наш текущий код уже использует `rule_set` (правильно).

### 3.2 Hiddify (`hiddify/hiddify-app/lib/singbox/model/singbox_rule.dart`, 803 B)

```dart
class SingboxRule {
  String? ruleSetUrl;      // ОДИН URL — по сути правило == rule-set ссылка
  String? domains;         // newline/comma-separated
  String? ip;              // CIDR
  String? port;            // port or port-range
  String? protocol;        // sniffed
  RuleNetwork network;     // tcp | udp | both
  RuleOutbound outbound;   // proxy | bypass | block
}
```

Hiddify делает **flat per-row rule** (одна строка UI = одно правило), что сильно упрощает UX. `domains` / `ip` / `port` — просто текст с newline-разделителями. Никаких process_name, source_*, invert, logical, override_address. Outbound — enum из 3 вариантов.

**Плюсы:** низкий порог входа, похоже на Clash Verge.
**Минусы:** не раскрывает мощь sing-box. Rule-сеты привязаны 1:1 к правилу (нельзя один rule-set использовать в нескольких правилах).

### 3.3 Clash Verge Rev (`clash-verge-rev/src/components/rule/rule-item.tsx`, 1.7 KB)

Только **read-only список** (типа `payload + type + proxy`). Никакого редактора — Clash Verge хранит правила в YAML, редактируются вне UI. Полезно как референс для визуализации списка (цветной бейдж по outbound, номер строки).

### 3.4 Loyalsoldier / v2ray-rules-dat (community rule-sets)

Стандарт де-факто, который уже используется в `previewConfig.ts`:
- `geoip-cn.srs`, `geoip-ru.srs`, `geosite-ads.srs` — Loyalsoldier mirror
- `geosite-geolocation-cn`, `geosite-cn`, `geosite-category-ads-all` — альтернативы (Loyalsoldier + meta-rules-dat)
- Источник: <https://github.com/Loyalsoldier/v2ray-rules-dat>

### 3.5 singbox-launcher — Go reference (архитектура, не код)

Хоть и не Tauri, **архитектура `core/`** прямая инспирация:

```
core/
  config/                # типы конфига + builder (≈ наш src-tauri/src/config/)
  config_service.go      # публичный API: Get/Set/Apply/Patch  (23KB)
  config_service_subscriptions.go  # вынесенные подписки  (22KB)
  rebuild.go             # "preview & apply" — точно наш generate_config
  template/              # встроенные шаблоны (≈ наши Presets)
  warp/                  # вынесенный helper-конфиг (как наш sing-box sidecar)
  events/                # pub/sub для UI
  state/                 # persisted state (≈ наш localStorage)
  services/              # межпроцессное взаимодействие
  uiservice/             # UI ↔ backend API
```

**Что заимствуем (структурно, не код):**
- `core/template/` → наш `src-tauri/src/presets/` для Loyalsoldier+meta-rules-dat
- `rebuild.go` (preview без apply) → у нас уже есть `previewConfig.ts` + `generate_config` (Rust)
- `config_service_subscriptions.go` (выделено из общего) → подтверждает наш план выделить `routing` из общего `config.rs`

**Что НЕ заимствуем:**
- Go concurrency model — у нас Rust + async Tauri
- `core/services/` IPC layer — у нас Tauri commands, другая модель

### 3.6 vpn-link-serde — Rust crate, НЕ замена нашим parser'ам

| Аспект | У нас (`src-tauri/src/parser/`) | `vpn-link-serde` 0.1.5 |
|---|---|---|
| Протоколы | VLESS, VMess, Trojan, SS, HY2, **TUIC** | VLESS, VMess, Trojan, SS, HY2 (**нет TUIC**) |
| V1 + V2 VMess | ✅ | ✅ |
| Reality / XTLS | ✅ | ✅ |
| SIP002 + base64 SS | ✅ | ✅ |
| hy2 obfs password | ✅ (через obfs.password) | ✅ (только auth) |
| Лицензия | MIT (наш код) | MIT (подключаемая) |
| Деплой | В нашем бинаре | +200KB к бинарю (deps: serde, base64, url, urlencoding) |

**Решение:** НЕ подключаем. У нас более полное покрытие (TUIC + obfs), и лишняя зависизь = лишний attack surface. **НО:** пишем regression-тесты, прогоняющие одинаковые ссылки через наш и их парсер — ловим edge cases.

### 3.7 Clash Verge Rev — паттерны организации

```
src/
  pages/        # routing-страницы (rules.tsx — 3KB, read-only)
  components/   # rule/, rule/provider-button.tsx, rule/rule-item.tsx
  services/     # api.ts (typed), cmds.ts (raw names), events.ts, delay.ts
  types/        # proxy-view.ts, global.d.ts
  hooks/        # React hooks
  providers/    # context providers
  utils/        # утилиты
  locales/      # i18n
```

**Что заимствуем:**
- `services/api.ts` (typed wrappers) + `services/cmds.ts` (raw Tauri command names) — наш `lib/api.ts` сейчас монолитный, можно отрефакторить. **НЕ в этом раунде** — отдельный PR.
- `components/rule/rule-item.tsx` — UI строки (1.7KB, уже взяли в первом раунде)
- `pages/rules.tsx` — подтверждение: **никто не делает полноценный editor в Tauri-GUI пространстве**. Наше конкурентное преимущество.

**Что НЕ заимствуем:**
- MUI → мы на Tailwind
- Routes через react-router → у нас собственная tab-система
- i18next → у нас пока нет i18n (можно добавить позже)

### 3.8 v2rayN + MK_XRAYchecker — не копал

`v2rayN` (C# WinForms) — другой стек. UX-паттерны Windows-приложения похожи на то, что мы делаем (sidebar, status bar, tray), но код не применим.

`MK_XRAYchecker` (Python) — тестирует прокси, не парсит. Не применимо к нашему стеку. Может пригодиться в будущем для rule analytics.

---

## 4. Предлагаемый design (3 scope-уровня)

### 🅰 MVP (≈ 1–2 дня работы, 0 новых зависимостей)

**Цель:** пользователь видит список правил, может добавить/удалить/включить, reorder через up/down кнопки.

**Что входит:**
- `RoutingOptions.rules: CustomRule[]` — массив с typed полями (см. §5)
- `RoutingOptions.rule_sets: RuleSetRef[]` — пользовательские rule-set'ы (URL, tag)
- `RoutingOptions.sniff: boolean` (default true) — был хардкод, теперь флаг
- Preset-кнопки: "Bypass LAN", "Bypass CN", "Bypass RU", "Block Ads", "Block QUIC" — добавляют соответствующие правила в массив (а не флипают флаг)
- Rule editor (modal): domain / domain_suffix / ip_cidr / port / network / outbound (proxy/direct/block)
- Per-row: drag-handle (up/down), toggle enabled, delete
- Persistence: текущий `localStorage["singbox-client.settings.v1"]` (bump v2 с миграцией)
- **Без**: drag-and-drop, импорт, process rules, rule_set URL editor, source_*

**Результат:** пользователь может сказать "reddit → direct, telegram → proxy, остальное → proxy", в отличие от текущих 5 бинарных опций.

### 🅱 Standard (≈ 3–4 дня, + dnd-kit или @dnd-kit/sortable)

**Дополнительно к MVP:**
- Drag-and-drop reorder (через `@dnd-kit/sortable`, ~30KB gz)
- Полный набор matcher'ов (domain_suffix, domain_keyword, ip_is_private, port_range, process_name, rule_set)
- Logical rules: `AND` / `OR` группы (1 уровень вложенности)
- Custom rule-set editor: tag + URL + format (auto от расширения) + update_interval
- Preset picker из списка Loyalsoldier (geoip-cn, geoip-ru, geosite-ads, geosite-geolocation-cn, geosite-category-ads-all, geosite-malware, geosite-phishing, geosite-cryptominers)
- JSON-превью автоматически обновляется (уже есть в ConfigTab)
- Кнопка "Copy rule JSON" для шаринга

**Результат:** покрывает 90% реальных юзкейсов учителя (work/school split, geo bypass, ad block, malware protection).

### 🅲 Power (≈ 5–7 дней, + нужны новые Tauri команды)

**Дополнительно к Standard:**
- Все matcher'ы из sing-box 1.14+ (interface_address, network_is_expensive на Win, preferred_by, source_mac_address, source_hostname)
- `route-options` actions (override_address, tls_fragment, tls_record_fragment, tls_spoof)
- `hijack-dns` и `resolve` actions
- `route.final` picker (не только из списка outbounds, но и из user-defined groups)
- Импорт/экспорт: Clash YAML rules, sing-box JSON rules
- Rule-set manager с inline-редактором (не только URL, но и `type: inline` с `rules: []`)
- Tauri-команды: `validate_rules(rules) -> Result<_, AppError>` — серверная валидация через sing-box
- Live rule-counter в UI (сколько правил сейчас активно)
- Валидация перед стартом sing-box (предотвращаем crash на кривом конфиге)

**Результат:** parity с Hiddify Next + расширения.

---

## 5. TypeScript types (proposed, для scope 🅰)

```ts
// src/lib/types.ts (дополнение)

/** Rule action — финальный исход матча */
export type RuleAction =
  | { kind: "route"; outbound: string }     // "proxy" | "direct" | "block" | "<server tag>"
  | { kind: "reject" }
  | { kind: "hijack-dns" }
  | { kind: "sniff"; sniffer?: string[]; timeout?: string }
  | { kind: "resolve"; server?: string; strategy?: string };

/** Один matcher — поле правила. Поддерживает и sing-box v1.14+ semantics */
export interface RuleMatchers {
  // network
  inbound?: string[];          // tag of inbound
  ip_version?: 4 | 6;
  network?: ("tcp" | "udp" | "icmp")[];
  auth_user?: string[];
  // domain
  domain?: string[];           // exact
  domain_suffix?: string[];    // .example.com
  domain_keyword?: string[];
  domain_regex?: string[];
  // ip
  ip_cidr?: string[];
  source_ip_cidr?: string[];
  ip_is_private?: boolean;
  source_ip_is_private?: boolean;
  // port
  port?: number[];             // exact
  port_range?: string[];       // "1000:2000" | ":3000" | "4000:"
  source_port?: number[];
  source_port_range?: string[];
  // sniff
  protocol?: string[];         // "http" | "tls" | "quic" | "dns" | "stun" | "ntp"
  client?: string[];           // since 1.10
  // process
  process_name?: string[];     // Win / Mac / Linux
  process_path?: string[];
  process_path_regex?: string[];
  // reference
  rule_set?: string[];         // tag of rule-set
  rule_set_ip_cidr_match_source?: boolean;  // 1.10+
  // flags
  invert?: boolean;
  enabled?: boolean;           // UI-only, default true
}

/** Custom rule в нашем конфиге */
export interface CustomRule {
  id: string;                  // uuid, для React key
  label?: string;              // "Telegram" | "Work" — UI only
  matchers: RuleMatchers;
  action: RuleAction;
}

/** Ссылка на rule-set (remote или inline) */
export interface CustomRuleSet {
  tag: string;                 // уникальный
  type: "remote" | "local" | "inline";
  format?: "source" | "binary";
  url?: string;                // remote
  path?: string;               // local
  rules?: HeadlessRule[];      // inline
  update_interval?: string;    // "1d", "12h", etc.
  enabled?: boolean;           // UI-only
}

export interface RoutingOptions {
  /** Кастомные правила в порядке приоритета (первое матчит первым) */
  rules: CustomRule[];
  /** Кастомные rule-set'ы (Loyalsoldier presets теперь тут, не в коде) */
  rule_sets: CustomRuleSet[];
  /** Глобальный sniff (default true) */
  sniff: boolean;
  /** Final outbound tag (default "proxy") */
  final_outbound: string;
  /** Включить auto_detect_interface для предотвращения routing loop под TUN */
  auto_detect_interface: boolean;
  /** Использовать default_domain_resolver (для DNS-leak защиты) */
  default_domain_resolver: string;
}
```

> ⚠️ Старые boolean-флаги (`bypass_lan`, `reject_ipv6`, `block_ads`, `bypass_cn`, `bypass_ru`, `block_quic`) **заменяются** на соответствующие `CustomRule` в `rules[]` (см. §6).

---

## 6. Migration plan (localStorage v1 → v2)

Старая структура:
```ts
{ routing: { bypass_lan: true, reject_ipv6: true, block_ads: false, ... } }
```

Новая структура:
```ts
{
  routing: {
    rules: [
      { id: "u1", matchers: { ip_cidr: ["10.0.0.0/8", "172.16.0.0/12", ...] }, action: { kind: "route", outbound: "direct" }, label: "Bypass LAN" },
      { id: "u2", matchers: { ip_version: 6 }, action: { kind: "reject" }, label: "Reject IPv6" },
      ...
    ],
    rule_sets: [...],
    sniff: true,
    final_outbound: "proxy",
    auto_detect_interface: true,
  }
}
```

**Миграция в `App.tsx` `loadSettings()`:**
- Меняем ключ на `singbox-client.settings.v2`
- Если есть v1 → конвертим boolean'ы в `CustomRule[]`, сохраняем v2, удаляем v1
- Если v2 уже есть — просто мерджим с дефолтами

**Лоялизация к старым флагам в previewConfig.ts:**
- Если `rules` пустой и нет v1, накатываем дефолтные 5 правил (sniff + LAN bypass + reject QUIC)
- Если `rules` пустой после миграции с v1 — оставляем пустым (не дублируем)

---

## 7. UI структура (scope 🅰/🅱)

```
App.tsx
└── Tabs
    └── Routing (NEW)
        ├── RoutingTab.tsx               ← контейнер
        │   ├── PresetPicker.tsx         ← "Add: Bypass LAN", "Add: Bypass CN", ...
        │   ├── RuleList.tsx             ← drag handle + rule item + add button
        │   │   └── RuleEditor.tsx       ← modal/drawer с matcher'ами + action
        │   ├── RuleSetsPanel.tsx        ← "Custom rule-sets" + Loyalsoldier presets
        │   └── FinalOutboundPicker.tsx  ← "Default goes to: proxy | direct"
        └── GeneralSettings.tsx          ← auto_detect_interface, sniff toggle
```

**Существующий код:**
- `ConfigBuilder.tsx` — убираем чекбоксы "Bypass LAN", "Block QUIC" и т.д. Оставляем только общие настройки (tunnel mode, port, DNS, clash_api, autostart).
- `previewConfig.ts` — `route.rules` собираем из `settings.routing.rules` (вместо хардкода). `route.rule_set` — из `settings.routing.rule_sets`.

---

## 8. Backend (Rust) изменения

`src-tauri/src/config/mod.rs` — текущий `GeneratorSettings` (serde-зеркало TS) надо расширить. Минимально для scope 🅰:

```rust
// snippet
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GeneratorSettings {
    pub tunnel_mode: TunnelMode,
    pub routing: RoutingOptions,
    pub clash_api: ClashApiOptions,
    // ...
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RoutingOptions {
    #[serde(default)]
    pub rules: Vec<CustomRule>,
    #[serde(default)]
    pub rule_sets: Vec<CustomRuleSet>,
    pub sniff: bool,
    pub final_outbound: String,
    pub auto_detect_interface: bool,
    pub default_domain_resolver: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CustomRule {
    pub id: String,
    pub label: Option<String>,
    pub matchers: serde_json::Value,  // free-form, совместимо с sing-box
    pub action: serde_json::Value,
    #[serde(default = "default_true")]
    pub enabled: bool,
}
```

`src-tauri/src/config/mod.rs::generate_config()` — текущая функция, которая собирает `Config`. Переписываем секцию `route` чтобы брать `rules` и `rule_sets` напрямую из `settings.routing` (а не хардкодить).

**Тесты:** `src-tauri/examples/verify_config.rs` — расширить golden-тест: добавить case с `rules: [...]`.

---

## 9. Открытые вопросы (нужен твой ответ)

1. **Scope:** ✅ выбран 🅱 Standard
2. **Presets:** ✅ оба (Loyalsoldier + meta-rules-dat) с переключателем в UI
3. **dnd-kit подходит?** Альтернативы: react-beautiful-dnd (deprecated 2024), react-dnd (старый API), нативный HTML5 drag (геморрой, но 0 KB). Я бы взял `@dnd-kit/sortable` (~30KB).
4. **Поведение `enabled: false`:** скрытое правило, или зачёркнутое в списке? (Hiddify скрывает, Clash Verge показывает серым)
5. **Rule editor — modal или inline (expandable row)?** Modal проще, inline — больше места, но сложнее.
6. **Где живёт кнопка "Reset rules to defaults":** в RoutingTab header или per-rule "reset this rule"?
7. **Migration UX:** silent при `loadSettings()` или показать toast "v1 settings upgraded, v0.1.0 routing was migrated to v2" один раз?
8. **Source организации пресетов в Rust:** статические константы в коде (как сейчас в `previewConfig.ts`) или динамическая загрузка из `src-tauri/presets/<file>.json`? Динамическая позволяет добавлять без перекомпиляции.

---

## 10. Что НЕ входит ни в один scope (deferred)

- Multi-user (правила per-user)
- Sync правил между устройствами
- GeoIP-lookup на лету для unknown IP
- Rule analytics ("это правило сработало 1.2k раз за час")
- Rule templates marketplace
- Визуальный graph-editor правил

---

## 11. Файлы, которые меняются

| Файл | Что |
|---|---|
| `src/lib/types.ts` | + `CustomRule`, `RuleMatchers`, `RuleAction`, `CustomRuleSet`, расширенный `RoutingOptions` |
| `src/components/ConfigBuilder.tsx` | убрать routing-блок, оставить tunnel/port/DNS/autostart |
| `src/components/ConfigTab.tsx` | перестать передавать routing в ConfigBuilder |
| `src/components/Routing/RoutingTab.tsx` (NEW) | главный контейнер |
| `src/components/Routing/RuleList.tsx` (NEW) | drag handle, reorder |
| `src/components/Routing/RuleEditor.tsx` (NEW) | editor матчеров и action |
| `src/components/Routing/PresetPicker.tsx` (NEW) | быстрая вставка Loyalsoldier preset |
| `src/components/Routing/RuleSetsPanel.tsx` (NEW) | управление rule-set'ами |
| `src/components/previewConfig.ts` | переписать секцию `route.rules` и `route.rule_set` |
| `src/App.tsx` | bump SETTINGS_KEY v1→v2, миграция, вкладка "Routing" |
| `src-tauri/src/config/mod.rs` | зеркалить новые TS-типы в Rust |
| `src-tauri/examples/verify_config.rs` | golden-тест с новой формой |
| `screenshots/08-routing-rules.png` (NEW) | скрин нового UI для README |

---

## 12. Открытые issue для будущего

- Source_MAC / source_hostname требует `find_neighbor: true` и `dhcp_lease_files` — на Windows **не работает** (только Linux/Mac). Не выкатываем в UI на Win.
- `package_name*` — Android-only. В веб-превью (vite без Tauri) не валидируется. Ставим флаг `"platform": "android"` в editor, на Win/Mac — disabled.
- `tls_spoof` / `tls_record_fragment` — нужен `elevated privileges` на Win. По умолчанию НЕ показываем, добавим под "Advanced" в scope 🅲.
