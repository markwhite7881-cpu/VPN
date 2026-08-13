<div align="center">

<img src="src-tauri/icons/icon.png" alt="Cloakwire" width="128" />

# Cloakwire

**Privacy-first GUI VPN client built on top of [sing-box](https://github.com/SagerNet/sing-box).**

Tauri 2 + React + TypeScript.

[![Release](https://img.shields.io/github/v/release/markwhite7881-cpu/cloakwire?include_prereleases&sort=semver&style=for-the-badge)](https://github.com/markwhite7881-cpu/cloakwire/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/markwhite7881-cpu/cloakwire/total?style=for-the-badge)](https://github.com/markwhite7881-cpu/cloakwire/releases/latest)
[![License](https://img.shields.io/github/license/markwhite7881-cpu/cloakwire?style=for-the-badge)](LICENSE)
[![Stars](https://img.shields.io/github/stars/markwhite7881-cpu/cloakwire?style=for-the-badge)](https://github.com/markwhite7881-cpu/cloakwire/stargazers)

</div>

> 🇬🇧 **English version:** [README.en.md](README.en.md)

---

## 🎯 Что это такое

**Cloakwire** — это минималистичный GUI VPN-клиент для Windows, который оборачивает мощь [sing-box](https://github.com/SagerNet/sing-box) в понятный интерфейс. Полный стек протоколов (VLESS, VMess, Trojan, Shadowsocks, Hysteria2, TUIC), авто-обновления и sing-box core в одном приложении.

**Главная идея:** вы просто выбираете программы, которые должны идти через VPN — остальное идёт напрямую. Никаких сложных правил, никакого конфига вручную.

---

## ✨ Почему Cloakwire

| | |
|---|---|
| 🚀 **Быстрый старт** | Подписка или share-link → готово за минуту |
| 🎯 **Per-app маршруты** | "Telegram через VPN, банк-клиент напрямую" — одной галочкой |
| 🔄 **Авто-обновления** | И оболочка, и sing-box core обновляются сами |
| 🛡️ **Полный sing-box** | VLESS, VMess, Trojan, SS, Hysteria2, TUIC |
| 🪶 **Лёгкий** | 7 MB portable, минимум зависимостей |
| 🎨 **Минималистичный UI** | Тёмная тема, один взгляд — и всё понятно |
| 🔓 **Open Source** | MIT, без трекеров, без телеметрии |

---

## 📥 Установка

### Скачать готовый билд (рекомендуется)

Перейдите в **[Releases → Latest](https://github.com/markwhite7881-cpu/cloakwire/releases/latest)** и скачайте:

- **`Cloakwire_1.0.x_x64-setup.exe`** (16 MB) — NSIS-установщик, рекомендуется
- **`Cloakwire_1.0.x_x64_en-US.msi`** (27 MB) — MSI для корпоративного деплоя
- **`Cloakwire-1.0.x-portable.exe`** (7.4 MB) — portable, без установки

> 💡 **Совет:** Windows SmartScreen может предупредить о неподписанном издателе. Нажмите "Подробнее" → "Выполнить в любом случае" — это нормально для open source без code-signing сертификата.

### Собрать из исходников

```powershell
# Требования: Node 20+, Rust 1.77+, Tauri CLI (npm i -g @tauri-apps/cli)
git clone https://github.com/markwhite7881-cpu/cloakwire.git
cd cloakwire
npm install
npm run tauri:build
# Артефакты: src-tauri\target\release\bundle\{msi,nsis}\
```

Portable exe: `src-tauri\target\release\cloakwire.exe`

---

## 🚀 Первый запуск

1. Запустите Cloakwire (потребуются права администратора для TUN-режима)
2. **Servers** → вставьте share-link (`vless://...`) или subscription URL → **Add**
3. **Routing** → добавьте программы в "Apps via VPN" (например `telegram.exe`)
4. **Config** → убедитесь, что **Tunnel mode = TUN**
5. **Home** → нажмите большую кнопку питания

Готово. Весь трафик выбранных программ идёт через VPN, остальное — напрямую.

---

## 🖼️ Скриншоты

> Скриншоты появятся скоро — соберём их на v1.0.3+.

А пока — словами:
- **Header:** логотип Cloakwire + версия + статус sing-box core
- **Home:** большая кнопка Start/Stop, live график трафика, обновления
- **Servers:** список подписок + manual profiles, drag-and-drop, latency ping
- **Routing:** top — два process-picker ("Apps via VPN" / "Apps direct"), ниже — Advanced (полный rule editor, drag-and-drop, presets, rule-sets)
- **Config:** режимы TUN/system proxy/both, DNS, preview сгенерированного `config.json`
- **Logs:** live tail sing-box с фильтром

---

## 🏗️ Архитектура

```
┌──────────────────────┐
│  React + Tailwind    │  ← tauri::invoke для всего
│  (src/)              │
└──────────┬───────────┘
           │ tauri::invoke  (typed commands)
┌──────────▼───────────┐
│  Rust + Tauri 2      │  ← CLI/sign/process/tun/log/updater
│  (src-tauri/src/)    │
└──────────┬───────────┘
           │ std::process::Command
┌──────────▼───────────┐
│  sing-box            │  ← VLESS/VMess/Trojan/SS/Hy2/TUIC, TUN, route
│  (binaries/)         │
└──────────────────────┘
```

**Слои:**
1. **Frontend** — React + TypeScript + Tailwind
2. **Tauri shell** — Rust-обёртка: команды процесса, авто-обновления, логирование, TUN-управление
3. **sing-box core** — сам VPN-протокол, конфиг генерируется из UI-структуры

---

## 🛠️ Стек

| Слой | Технология |
|------|------------|
| Shell | **Tauri 2** (Rust + WebView) |
| UI | **React 18** + **TypeScript 5** |
| Стили | **Tailwind CSS 3** + shadcn-style design tokens |
| Ядро VPN | [**sing-box**](https://github.com/SagerNet/sing-box) 1.10+ (sidecar) |
| Drag-and-drop | **@dnd-kit/sortable** |
| Иконки | **lucide-react** |
| Авто-обновления | `tauri-plugin-updater` + `tauri-plugin-process` (Ed25519 minisign) |
| Process enum | `sysinfo` crate |
| Подпись | Custom `tauri-signer` crate (см. ниже) |

---

## 📦 Этапы разработки

| Этап | Статус | Что сделано |
|------|--------|-------------|
| 1. Tauri scaffold + sidecar | ✅ | Spawn/stop, log streaming, health-check, default config |
| 2. Парсер протоколов | ✅ | vless / vmess / trojan / ss / hy2 / tuic |
| 3. Генератор конфига | ✅ | TUN inbound, route rules, selector/urltest, `sing-box check` clean |
| 4. Clash API | ✅ | list/select/test_delay over HTTP |
| 5. Список серверов + график трафика | ✅ | WebSocket traffic stream, SVG sparklines |
| 6. Авто-обновление подписок | ✅ | Fetch + parse + per-line errors + auto-refresh tick |
| 7. Routing + автозапуск | ✅ | Geosite/geoip presets (ads/CN/RU/QUIC) + Windows autostart |
| 8. v1.0.0 + авто-обновление | ✅ | Tauri updater (Ed25519), sing-box core auto-update |

---

## 🔐 Безопасность

- **Ed25519 minisign** для подписи обновлений (Tauri updater + наш `tauri-signer`)
- **Без телеметрии**, без аналитики, без "phone home" кроме проверки обновлений
- **Локальные настройки** в `%APPDATA%\ru.classquiz.singbox\` (Tauri `app_data_dir`)
- **Безопасный WebView**
- **Open source** — каждая строчка кода видна

---

## 🧪 Smoke-тест (Stage 1)

Минимальная проверка, что sing-box живёт:

```powershell
# Запустите Cloakwire один раз — он распакует sing-box в %APPDATA%\ru.classquiz.singbox\binaries\
Get-Process sing-box
# Должен показать запущенный процесс
```

---

## 🤝 Contributing

PR-ы приветствуются. Перед большим изменением лучше открыть issue — обсудим.

- **Code style:** `cargo fmt` для Rust, `prettier` для TS/TSX (`npm run build` проверяет)
- **Tests:** `cargo test --lib` (56 unit-тестов на конфиг-генератор и парсер)
- **Commits:** conventional commits помогают (`feat:`, `fix:`, `chore:`)

---

## 🛠️ Почему кастомный подписант

`npx tauri signer sign` подвисает на Windows после "Signing without password." (TTY-детект в tauri-cli сломан). Мы собрали `tauri-signer` crate, который использует тот же `minisign = "0.9"` что и апстрим, но без интерактивного prompt'а. Исходник: `src-tauri/crates/tauri-signer/`.

---

## 📜 Лицензия

[MIT](LICENSE) — делайте что хотите, но упомяните авторов.

---

## 🙏 Благодарности

- [**SagerNet/sing-box**](https://github.com/SagerNet/sing-box) — самый быстрый и гибкий VPN-протокол из существующих
- [**Tauri**](https://tauri.app) — обёртка для десктопных приложений, которая не подводит

---

<div align="center">

**[⬇ Скачать последнюю версию](https://github.com/markwhite7881-cpu/cloakwire/releases/latest)** · **[🐛 Сообщить о баге](https://github.com/markwhite7881-cpu/cloakwire/issues)** · **[⭐ Поставить звезду](https://github.com/markwhite7881-cpu/cloakwire)**

Made with care for people who value their privacy.

</div>
