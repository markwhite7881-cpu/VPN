# Cloakwire Android 1.3.0 Rebuild Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce a production-signed ARM64 Android APK whose internal version is `1.3.0` / `1003000`, while preserving the previously verified Android application behavior and the desktop `v1.3.0` security/runtime baseline.

**Architecture:** Keep the imported Android Kotlin/VPN and mobile React layer functionally unchanged. Reconcile only the shared Tauri/Rust and ACL boundaries required to run that layer on Android, and add a deterministic Android metadata generation/check step. The build pipeline signs only the resulting release APK with the existing local certificate and compares package metadata, ABI, signing certificate, and selected application resources against the verified `1.2.0` reference before upload.

**Tech Stack:** Tauri 2, Rust 2021, Kotlin/Gradle Android, React/TypeScript, Android SDK Build Tools, Java 17, PowerShell 5.1+, APK Signature Scheme v2/v3.

## Global Constraints

- Do not modify Android VPN behavior, mobile UI flows, subscriptions, routing rules, or persisted application data format except where a compiler/runtime error proves an Android bridge change is required.
- Preserve the desktop updater/security/runtime code at `574e3f4ee5216265db472f44102f8d5fb6b58785`; Android-specific compilation must be feature- or target-gated.
- Keep Android architecture ARM64-only: `abiList=arm64-v8a`, `archList=arm64`, `targetList=aarch64`.
- The final APK must report package `ru.classquiz.singbox`, `versionName=1.3.0`, and `versionCode=1003000`.
- Do not modify the APK after signing. Build metadata belongs in source/configuration before Gradle packages the APK.
- Use only the existing Android production certificate. Do not generate or replace a certificate; do not print or commit passwords, keystores, AARs, APKs, generated build output, signing env files, or `release-staging/`.
- Never move or overwrite the published `v1.3.0` tag or Windows assets. Upload the Android APK only after all verification gates pass.
- Android release notes must identify the file as ARM64-only and state its own internal Android version `1.3.0`.

---

## File Structure

| File | Responsibility |
|---|---|
| `src-tauri/src/lib.rs` | Target-gated registration of desktop plugins and Android `VpnPlugin`; shared command handler remains explicit. |
| `src-tauri/src/commands.rs` | Android-safe application/private configuration path handling if required by the existing mobile bridge. |
| `src-tauri/src/process.rs` | Minimal Android-safe process/config behavior only if required by compilation or invocation tests. |
| `src-tauri/Cargo.toml` | Target-scoped dependencies required by Android only; desktop autostart must not be pulled into Android. |
| `src-tauri/build.rs` | Include the inlined Android VPN permission manifest if required by Tauri's generated ACL. |
| `src-tauri/capabilities/default.json` | Desktop capabilities only; no Android-only VPN grant required here. |
| `src-tauri/capabilities/mobile.json` | Android main-webview capability with `vpn:default` and only required core permissions. |
| `src-tauri/permissions/vpn/default.toml` | Existing Kotlin plugin permissions, retained without behavior changes. |
| `src-tauri/tauri.android.conf.json` | Android-only Tauri overlay. |
| `scripts/sync-android-version.ps1` | Deterministically derives generated Android version metadata from canonical project version and validates the expected code. |
| `scripts/verify-android-apk.ps1` | Verifies APK package/version/ABI/signature/certificate and compares stable application metadata to the trusted reference APK. |
| `scripts/sign-android-release.ps1` | Reads local environment values, aligns/signs a supplied release APK through Java `apksigner.jar`, and never emits secrets. |
| `docs/superpowers/plans/2026-08-17-cloakwire-android-1.3.0-rebuild.md` | This controlled rebuild plan. |

## Task 1: Establish non-regression inventory before reconciliation

**Files:**
- Create: `scripts/verify-android-apk.ps1`
- Test input: trusted `Cloakwire_1.2.0_arm64-v8a.apk` outside Git

**Interfaces:**
- Consumes: `-ApkPath`, `-ReferenceApkPath`, `-ExpectedVersionName`, `-ExpectedVersionCode`, `-ExpectedPackage`, `-ExpectedAbi`, and `-ExpectedCertificateSha256`.
- Produces: non-zero exit on any mismatch; a redacted report of version, package, ABI, signing schemes, certificate digest, file hash, and stable resource inventory.

- [ ] **Step 1: Capture the trusted reference inventory without modifying it**

Run:

```powershell
$ErrorActionPreference = 'Stop'
$reference = 'C:\Users\Алексей\.minimax\v2\assets\2026\08\17\02-21-16-783-asset_20260817-022116-783_07711c61dec3_8ae9cec2-Cloakwire_1.2.0_arm64-v8a.apk'
Get-FileHash -LiteralPath $reference -Algorithm SHA256
```

Expected: hash is `07711C61DEC3BB9EFA084233E50C79096C6A700DE7D1DBF54FEED82644CBAFDA`.

- [ ] **Step 2: Write a failing verifier invocation for the future release version**

Run:

```powershell
$ErrorActionPreference = 'Stop'
& .\scripts\verify-android-apk.ps1 `
  -ApkPath $reference `
  -ReferenceApkPath $reference `
  -ExpectedVersionName '1.3.0' `
  -ExpectedVersionCode 1003000 `
  -ExpectedPackage 'ru.classquiz.singbox' `
  -ExpectedAbi 'arm64-v8a' `
  -ExpectedCertificateSha256 '07c14843f191d7f85df335709e0859887bc790f9b0074b98481246638dee2ca1'
```

Expected: FAIL only on version mismatch, demonstrating that the verifier rejects a `1.2.0` APK claimed as `1.3.0`.

- [ ] **Step 3: Implement the verifier with no secret inputs**

Implement a PowerShell script that uses the latest installed Android build-tools plus Java `apksigner.jar` and `aapt.exe`. It must:

```powershell
param(
  [Parameter(Mandatory)] [string] $ApkPath,
  [Parameter(Mandatory)] [string] $ReferenceApkPath,
  [Parameter(Mandatory)] [string] $ExpectedVersionName,
  [Parameter(Mandatory)] [int] $ExpectedVersionCode,
  [Parameter(Mandatory)] [string] $ExpectedPackage,
  [Parameter(Mandatory)] [string] $ExpectedAbi,
  [Parameter(Mandatory)] [string] $ExpectedCertificateSha256
)
```

Parse `aapt dump badging` for package/version and `native-code`, parse `java -jar apksigner.jar verify --verbose --print-certs` for v2/v3 and signer SHA-256. Require the reference and candidate to have identical manifest package, native ABI, signer digest, application id, and resource entry names excluding build metadata. Require the candidate version to equal supplied expected values. Print only non-secret facts and SHA-256 file hash.

- [ ] **Step 4: Demonstrate the failing reference check remains meaningful**

Run the command from Step 2 again.

Expected: FAIL with an explicit `versionName expected 1.3.0, got 1.2.0` or equivalent message; no file is changed.

- [ ] **Step 5: Commit the verifier only**

```powershell
$ErrorActionPreference = 'Stop'
git add -- scripts/verify-android-apk.ps1
git commit -m 'test(android): add release APK verification gate'
```

Expected: no APK, signing input, or generated build output is staged.

## Task 2: Make Android plugin and capabilities compile without desktop behavior changes

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/build.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/capabilities/default.json`
- Create: `src-tauri/capabilities/mobile.json`
- Modify: `src-tauri/tauri.android.conf.json` if it needs to enumerate the mobile capability
- Test: Rust check for Android target and desktop `cargo check`

**Interfaces:**
- Consumes: Kotlin class `ru.classquiz.singbox.vpn.VpnPlugin` and plugin permission `vpn:default`.
- Produces: `fn vpn_mobile_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry>` for Android only; desktop builder still registers shell, opener, dialog, and autostart exactly as before.

- [ ] **Step 1: Add a focused failing Android compilation gate**

Run:

```powershell
$ErrorActionPreference = 'Stop'
$env:CARGO_HOME = 'C:\Users\Алексей\.cargo'
$env:RUSTUP_HOME = 'C:\Users\Public\cwdev\rustup-home'
$env:Path = 'C:\Users\Алексей\.cargo\bin;' + $env:Path
Set-Location '.\src-tauri'
cargo check --target aarch64-linux-android
```

Expected: FAIL before reconciliation because desktop-only plugin/dependency and missing Android plugin registration are not platform-safe.

- [ ] **Step 2: Make autostart an explicit desktop-only dependency and builder registration**

Move `tauri-plugin-autostart` into a non-Android target dependency section. In `lib.rs`, place both `MacosLauncher` import and `.plugin(tauri_plugin_autostart::init(...))` behind `#[cfg(desktop)]`; keep its current launch arguments unchanged.

- [ ] **Step 3: Add Android plugin registration behind `target_os = "android"`**

Add:

```rust
#[cfg(target_os = "android")]
fn vpn_mobile_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::new("vpn")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            {
                api.register_android_plugin(
                    "ru.classquiz.singbox.vpn",
                    "VpnPlugin",
                )?;
            }
            Ok(())
        })
        .build()
}
```

Register it only inside the Android builder path. Keep the existing desktop command set and updater commands unchanged.

- [ ] **Step 4: Separate desktop and mobile capabilities**

Keep desktop `shell` and `autostart` permissions in the desktop/default capability. Create `mobile.json` containing `core:default`, the existing window/core permissions actually used by `MobileApp`, `dialog:default`, `opener:default` only if mobile imports require them, and `vpn:default`. Do not grant `shell:*` or `autostart:*` to Android.

- [ ] **Step 5: Register the VPN permission manifest at build time**

Use Tauri's inlined-plugin manifest support in `build.rs` so the generated `acl-manifests.json` includes a non-null `vpn.default_permission`. The manifest must resolve `vpn:default` through top-level `[default]` in `src-tauri/permissions/vpn/default.toml`.

- [ ] **Step 6: Run both platform compile gates**

Run:

```powershell
$ErrorActionPreference = 'Stop'
Set-Location '.\src-tauri'
cargo fmt --check
cargo check
cargo check --target aarch64-linux-android
```

Expected: both checks pass without warnings; generated ACL reports `vpn.default_permission` is non-null for Android.

- [ ] **Step 7: Commit only source/configuration reconciliation**

```powershell
$ErrorActionPreference = 'Stop'
git add -- src-tauri/Cargo.toml src-tauri/build.rs src-tauri/src/lib.rs src-tauri/capabilities/default.json src-tauri/capabilities/mobile.json src-tauri/tauri.android.conf.json src-tauri/permissions/vpn/default.toml
git commit -m 'fix(android): isolate mobile plugin and capabilities'
```

Expected: no generated Android package, AAR, keystore, or Gradle output is staged.

## Task 3: Synchronize Android build metadata from the canonical release version

**Files:**
- Create: `scripts/sync-android-version.ps1`
- Modify: `src-tauri/tauri.android.conf.json` only if needed for Tauri generation inputs
- Test: generated `src-tauri/gen/android/app/tauri.properties`

**Interfaces:**
- Consumes: canonical version from `src-tauri/tauri.conf.json` and `package.json`.
- Produces: `tauri.android.versionName=<version>` and integer `tauri.android.versionCode=<major><minor as 3 digits><patch as 3 digits>`; `1.3.0` becomes `1003000`.

- [ ] **Step 1: Write failing mismatch check**

Run:

```powershell
$ErrorActionPreference = 'Stop'
& .\scripts\sync-android-version.ps1 -CheckOnly
```

Expected: FAIL while generated `tauri.properties` is absent or contains `1.2.0` / `1002000`.

- [ ] **Step 2: Implement deterministic version parsing and validation**

Read `package.json` and `src-tauri/tauri.conf.json`, require exactly equal `major.minor.patch` values, and derive:

```powershell
$versionCode = ($major * 1000000) + ($minor * 1000) + $patch
```

In non-check mode, invoke the project’s Tauri Android generation command/overlay, then require `src-tauri/gen/android/app/tauri.properties` to contain only the expected `tauri.android.versionName` and `tauri.android.versionCode` values. Never mutate an APK.

- [ ] **Step 3: Run the generator/check for 1.3.0**

Run:

```powershell
$ErrorActionPreference = 'Stop'
& .\scripts\sync-android-version.ps1
& .\scripts\sync-android-version.ps1 -CheckOnly
```

Expected: output confirms `1.3.0` and `1003000`.

- [ ] **Step 4: Commit version synchronization logic only**

```powershell
$ErrorActionPreference = 'Stop'
git add -- scripts/sync-android-version.ps1 src-tauri/tauri.android.conf.json
git commit -m 'build(android): synchronize release version metadata'
```

Expected: generated `tauri.properties` remains ignored unless project policy expressly tracks it.

## Task 4: Build and validate unchanged Android functionality before signing

**Files:**
- Modify: no product sources unless Task 2/3 compilation identifies a documented, minimal bridge defect
- Build inputs: untracked local `libbox.aar` and existing Android SDK/NDK toolchain
- Build output: ignored Gradle/Tauri release APK

**Interfaces:**
- Consumes: reconciled source, checked `tauri.properties`, ARM64-only Gradle properties, and trusted local `libbox.aar`.
- Produces: an unsigned ARM64 release APK with package `ru.classquiz.singbox` and `1.3.0`/`1003000` metadata.

- [ ] **Step 1: Verify build prerequisites without exposing credentials**

Run a PowerShell prerequisite check that confirms the existence of `libbox.aar`, Android SDK, NDK, Java, Gradle wrapper, and required Rust target. Print paths and tool versions only; do not inspect password files or keystores.

- [ ] **Step 2: Build the ARM64 release APK**

Run the project’s Tauri Android release command with only the `aarch64` target, preserving `src-tauri/gen/android/gradle.properties` ARM64 settings. Capture logs in ignored `release-staging/`; terminate on non-zero exit code.

Expected: exactly one unsigned release APK under `src-tauri/gen/android/app/build/outputs/apk/**/release/`.

- [ ] **Step 3: Validate the unsigned APK's internal metadata and non-signing invariants**

Run `aapt dump badging` to require package `ru.classquiz.singbox`, `versionName='1.3.0'`, `versionCode='1003000'`, and `native-code: 'arm64-v8a'`. Compare resource entry names and Android application component names to the trusted `1.2.0` reference. Any difference outside expected generated version/build metadata is a blocker for review.

- [ ] **Step 4: Run Kotlin unit tests and frontend build**

Run:

```powershell
$ErrorActionPreference = 'Stop'
Set-Location '.\src-tauri\gen\android'
.\gradlew.bat :app:testReleaseUnitTest
Set-Location '..\..\..'
npm run build
```

Expected: Kotlin routing-policy tests and frontend production build pass.

## Task 5: Sign, independently verify, and publish the Android APK

**Files:**
- Create: `scripts/sign-android-release.ps1`
- Modify: GitHub Release `v1.3.0` only after all local gates pass
- Output: untracked `release-staging/v1.3.0/Cloakwire_1.3.0_arm64-v8a.apk`

**Interfaces:**
- Consumes: `CLOAKWIRE_ANDROID_KEYSTORE_PATH`, `CLOAKWIRE_ANDROID_KEYSTORE_PASSWORD`, `CLOAKWIRE_ANDROID_KEY_ALIAS`, `CLOAKWIRE_ANDROID_KEY_PASSWORD`, and a verified unsigned APK path.
- Produces: signed APK plus its SHA-256, verified using v2 and v3 and the established certificate digest.

- [ ] **Step 1: Write a failing signing-environment gate**

Run the signing script without setting values and require it to fail before opening any input:

```powershell
$ErrorActionPreference = 'Stop'
Remove-Item Env:CLOAKWIRE_ANDROID_KEYSTORE_PATH -ErrorAction SilentlyContinue
& .\scripts\sign-android-release.ps1 -InputApkPath '.\does-not-exist.apk' -OutputApkPath '.\release-staging\invalid.apk'
```

Expected: fails with a list of missing variable names only; no secrets and no output APK.

- [ ] **Step 2: Implement secret-safe Java signing**

The script must require all four variables, check keystore path exists and is outside the repository, copy/aligned output to a fresh supplied output path, and invoke Java directly with `apksigner.jar` (not `apksigner.bat`, which can corrupt shell-special passwords). Do not echo arguments, environment values, command line, or exception details that include secrets. Enable v2 and v3; v1 and v4 remain disabled unless Android compatibility testing explicitly changes the decision.

- [ ] **Step 3: Sign the release artifact from the local environment**

Set variables locally in the current PowerShell session, never in source control, then run:

```powershell
$ErrorActionPreference = 'Stop'
& .\scripts\sign-android-release.ps1 `
  -InputApkPath '<verified unsigned release APK>' `
  -OutputApkPath '.\release-staging\v1.3.0\Cloakwire_1.3.0_arm64-v8a.apk'
```

Expected: one signed release APK is produced and no sensitive value is printed.

- [ ] **Step 4: Perform independent release verification**

Run:

```powershell
$ErrorActionPreference = 'Stop'
& .\scripts\verify-android-apk.ps1 `
  -ApkPath '.\release-staging\v1.3.0\Cloakwire_1.3.0_arm64-v8a.apk' `
  -ReferenceApkPath 'C:\Users\Алексей\.minimax\v2\assets\2026\08\17\02-21-16-783-asset_20260817-022116-783_07711c61dec3_8ae9cec2-Cloakwire_1.2.0_arm64-v8a.apk' `
  -ExpectedVersionName '1.3.0' `
  -ExpectedVersionCode 1003000 `
  -ExpectedPackage 'ru.classquiz.singbox' `
  -ExpectedAbi 'arm64-v8a' `
  -ExpectedCertificateSha256 '07c14843f191d7f85df335709e0859887bc790f9b0074b98481246638dee2ca1'
```

Expected: PASS; output reports v2/v3 valid, same certificate, target package, ARM64 ABI, and a SHA-256 file hash.

- [ ] **Step 5: Commit signing automation without signing material**

```powershell
$ErrorActionPreference = 'Stop'
git add -- scripts/sign-android-release.ps1
git commit -m 'build(android): add local production signing gate'
git status --short
```

Expected: script only; keystore, APKs, AARs, signing config, and staging remain untracked/ignored.

- [ ] **Step 6: Upload only the verified APK to GitHub Release `v1.3.0`**

Upload `Cloakwire_1.3.0_arm64-v8a.apk` to the existing release without altering its tag/target. Append a release-note section:

```markdown
### Android

- `Cloakwire_1.3.0_arm64-v8a.apk` — Android 1.3.0 (`versionCode` 1003000), production-signed, for ARM64 (`arm64-v8a`) devices only.
```

Expected: GitHub API returns the exact uploaded name, byte size, and SHA-256 digest matching the local verifier output.

- [ ] **Step 7: Verify the uploaded asset independently**

Download or query the GitHub asset metadata and compare byte size and SHA-256 digest with the local output. Confirm the release still targets `574e3f4ee5216265db472f44102f8d5fb6b58785` and Windows assets remain present.

## Plan Self-Review

- Spec coverage: Task 1 introduces a failure-first verifier and reference inventory; Task 2 isolates shared mobile/desktop compilation and ACL; Task 3 makes Android `1.3.0` metadata deterministic; Task 4 validates the unsigned functional build without changing application behavior; Task 5 signs, verifies, and publishes only after all gates pass.
- Placeholder scan: no deferred implementation placeholders; all release/version values, paths, interfaces, expected outputs, and commands are explicit.
- Type consistency: the signing environment uses the same four `CLOAKWIRE_ANDROID_*` names in constraints and Task 5. Verifier arguments and expected package/version/ABI/certificate values match throughout.
