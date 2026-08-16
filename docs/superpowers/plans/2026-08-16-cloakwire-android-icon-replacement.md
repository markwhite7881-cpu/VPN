# Android Launcher Icon Replacement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Cloakwire’s Android launcher icon with the supplied 1024×1024 PNG and verify it in a newly built APK on the connected Android device.

**Architecture:** The Android manifest already references `@mipmap/ic_launcher`. Replace the existing density-specific `ic_launcher.png` and `ic_launcher_round.png` bitmap resources with high-quality square downscales of the supplied source image. The app’s Kotlin, Rust, TypeScript, manifest and VPN configuration remain unchanged.

**Tech Stack:** Android resource packaging through Gradle/Tauri, PowerShell, .NET `System.Drawing`, Android Debug Bridge (adb).

## Global Constraints

- Use the supplied PNG located at `C:\Users\Алексей\.minimax\v2\assets\2026\08\16\18-41-59-382-asset_20260816-184159-382_93f023adfc0a_75ac7a8c-icon.png` as the sole icon source.
- Keep the 1024×1024 source square; resize without cropping or adding a background.
- Replace only `ic_launcher.png` and `ic_launcher_round.png` under Android `mipmap-*` resource directories.
- Do not alter VPN behavior, mobile UI, manifest, package identity, or dependency declarations.
- Build only arm64 Android debug APK using the established ASCII path environment.
- Target device serial is `3B15AV0166300000` for installation and verification.

---

### Task 1: Generate density-specific launcher resources

**Files:**
- Modify: `src-tauri/gen/android/app/src/main/res/mipmap-mdpi/ic_launcher.png`
- Modify: `src-tauri/gen/android/app/src/main/res/mipmap-hdpi/ic_launcher.png`
- Modify: `src-tauri/gen/android/app/src/main/res/mipmap-xhdpi/ic_launcher.png`
- Modify: `src-tauri/gen/android/app/src/main/res/mipmap-xxhdpi/ic_launcher.png`
- Modify: `src-tauri/gen/android/app/src/main/res/mipmap-xxxhdpi/ic_launcher.png`
- Modify: matching `ic_launcher_round.png` files in the same five directories.

**Interfaces:**
- Consumes: attached 1024×1024 PNG.
- Produces: Android launcher icon bitmaps at 48, 72, 96, 144 and 192 px.

- [ ] **Step 1: Confirm the supplied source is a square ARGB image**

Run a PowerShell `System.Drawing.Image` inspection and expect `1024x1024; PixelFormat=Format32bppArgb`.

- [ ] **Step 2: Resize the source into all launcher density resources**

Run a PowerShell script using `System.Drawing.Bitmap` and high-quality bicubic interpolation. Map density folders to pixels: `mdpi=48`, `hdpi=72`, `xhdpi=96`, `xxhdpi=144`, `xxxhdpi=192`. Save each generated bitmap to both `ic_launcher.png` and `ic_launcher_round.png` in its corresponding directory.

- [ ] **Step 3: Verify generated dimensions and changed-resource list**

Inspect every generated PNG using `System.Drawing.Image` and confirm that each launcher and round icon exactly matches its designated pixel size. Use `git status --short` to ensure only icon resources were created or modified by this task.

### Task 2: Build, install and verify the icon APK

**Files:**
- Uses built artifact: `src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk`

**Interfaces:**
- Consumes: density-specific launcher icons from Task 1.
- Produces: installed Android debug app showing the new launcher icon resource.

- [ ] **Step 1: Build the arm64 debug APK**

Run the established ASCII-path environment:

```powershell
$env:ANDROID_NDK_HOME = 'C:\Users\Public\cwdev\ndk'
$env:NDK_HOME = $env:ANDROID_NDK_HOME
$env:RUSTUP_HOME = 'C:\Users\Public\cwdev\rustup-home'
$env:CARGO_HOME = 'C:\Users\Алексей\.cargo'
$env:CARGO_TARGET_DIR = 'C:\Users\Public\cwdev\target'
$env:Path = 'C:\Users\Алексей\.cargo\bin;' + $env:Path
$ErrorActionPreference = 'Continue'
$PSNativeCommandUseErrorActionPreference = $false
npx tauri android build --apk --debug --target aarch64
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
```

Expected: successful Android build; the known Vite large-chunk advisory is non-fatal.

- [ ] **Step 2: Install the built APK on the specified device**

```powershell
& 'C:\Users\Алексей\AppData\Local\Android\Sdk\platform-tools\adb.exe' -s '3B15AV0166300000' install -r 'src-tauri\gen\android\app\build\outputs\apk\universal\debug\app-universal-debug.apk'
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
```

Expected: `Success`.

- [ ] **Step 3: Verify the package’s launcher icon resource and process launch**

Use `adb shell cmd package resolve-activity --brief ru.classquiz.singbox` and `adb shell monkey -p ru.classquiz.singbox 1`, then confirm the application process is present. This verifies the installed package and activity use the updated APK; visual launcher confirmation remains available to the user on the device.

- [ ] **Step 4: Check final diff quality**

Run `git diff --check` and inspect `git status --short`. Expected: no whitespace errors; unrelated pre-existing working-tree changes remain untouched.
