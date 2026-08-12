Singbox Client v0.1.0 — portable build
======================================

Quick start:
  1. Extract this archive to any folder.
  2. Run "Singbox Client.exe".
  3. In the app: Subscriptions -> paste your URL -> Fetch -> Connect.

Files in this archive:
  - Singbox Client.exe                              main app (Tauri 2)
  - sing-box-x86_64-pc-windows-msvc.exe            sing-box sidecar
  - libcronet.dll                                   Chromium networking
  - WebView2Loader.dll                              resolved at runtime from system

Requirements:
  - Windows 10 21H2 or newer (WebView2 runtime is preinstalled since Win11 22H2
    and on most Win10 machines via Edge updates). If missing, install:
    https://developer.microsoft.com/microsoft-edge/webview2/

Source code & issues:
  https://github.com/markwhite7881-cpu/VPN

License: MIT
