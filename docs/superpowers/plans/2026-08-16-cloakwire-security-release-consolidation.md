# Cloakwire Security, Release, and Repository Consolidation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close updater trust-boundary flaws, make platform update/release paths reliable, correct mobile Auto selection, and converge all retained project history into `main` before deleting obsolete remote branches.

**Architecture:** The Rust backend becomes the sole authority that refetches, validates, and installs update artifacts; the frontend carries only UI state and an optional expected version. The release system validates a staged artifact set before writing a manifest, while macOS emits both first-install and updater artifacts. Git consolidation occurs last, after integration and verification establish `main` as the only surviving branch.

**Tech Stack:** Rust 2021, Tauri 2, reqwest/rustls, minisign-compatible updater signatures, React 18/TypeScript, PowerShell 5.1+, GitHub Actions, GitHub CLI.

## Global Constraints

- Do not accept a download URL from WebView IPC for app-shell or sing-box installation.
- Never download, replace, or execute an updater artifact before backend integrity validation.
- Keep private keys, tokens, build output, generated `.tsbuildinfo`, and mobile build artifacts out of Git.
- Preserve the custom `reqwest` + rustls app-updater fetch path; do not revert to schannel/WinINet plugin fetching.
- Do not publish a release, delete existing tags, install missing system tools, or delete a remote branch until all required checks pass.
- Use PowerShell syntax for every Windows command and UTF-8-safe file operations.
- `main` is the target long-lived branch; remote feature branches are deleted only after their useful commits are reachable from `main`.

---

## File Structure

| File | Responsibility |
|---|---|
| `src-tauri/src/app_update.rs` | Fetch and parse signed manifest, validate update artifact origin/signature, preserve platform package kind, launch only verified installers. |
| `src-tauri/src/updates.rs` | Fetch trusted sing-box release metadata, select and verify platform asset/checksum, stage and atomically replace core. |
| `src-tauri/src/commands.rs` | Narrow public Tauri commands to no longer accept artifact URLs. |
| `src/lib/api.ts` | Align TypeScript IPC wrappers with narrowed Rust command signatures. |
| `src/components/UpdateCard.tsx` | Keep only display data and request installation without forwarding URLs. |
| `src-tauri/src/process.rs` | Make reset cleanup remove system proxy before state is discarded. |
| `src/mobile/MobileApp.tsx` | Derive server selection from `default_outbound`, preserving Auto state. |
| `src/mobile/screens/HomeScreen.tsx` | Render explicit Auto selection details when no profile is pinned. |
| `src-tauri/Cargo.toml` | Add only the cryptographic verification dependency required by the implementation. |
| `scripts/release.ps1` | Fail on build errors, isolate artifacts, verify versions and signatures before manifest output. |
| `scripts/write-latest-json.ps1` | Parameterize artifact/manifest generation and enforce full expected platform coverage. |
| `.github/workflows/release-macos.yml` | Build, package, sign, and validate Darwin updater archives as well as DMGs. |
| `.github/workflows/release-android.yml` | Pin action/source revisions and preserve Android release behavior. |
| `.github/workflows/release-validation.yml` | Validate staged release assets and `latest.json` independently of platform builders. |
| `scripts/validate-release.ps1` | Reusable artifact/manifest validation callable locally and in CI. |
| `src-tauri/src/*_tests.rs` or inline `#[cfg(test)]` modules | Rust regression coverage for manifest/origin/package/update selection behavior. |
| `src/mobile/**/*.test.tsx` (or established frontend test location) | Regression test for default Auto selection once test runner is established. |

## Task 1: Establish an isolated integration baseline and classify branch history

**Files:**
- Create: `docs/superpowers/plans/2026-08-16-cloakwire-security-release-consolidation.md` (this plan)
- Modify: no product files

**Interfaces:**
- Consumes: remote branches `origin/main`, `origin/android/v1.2.0-vpn-release`, `origin/linux/fix-singbox-auto-update-platform`, `origin/linux/macos-build`, `origin/linux/runtime-fixes`.
- Produces: a verified commit mapping stating which branch tips and unique commits must become ancestors of the integration branch.

- [ ] **Step 1: Refresh remote references without changing working files**

Run:

```powershell
$ErrorActionPreference = 'Stop'
Set-Location 'C:\Users\Алексей\.minimax-agent\projects\singbox-client'
git fetch --prune origin
git status --short
git log --graph --oneline --decorate --all -n 120
```

Expected: the known local uncommitted files remain unmodified; graph contains all five remote refs.

- [ ] **Step 2: Write the failing integration invariant as commands**

Run:

```powershell
$ErrorActionPreference = 'Stop'
$branches = @(
  'origin/android/v1.2.0-vpn-release',
  'origin/linux/fix-singbox-auto-update-platform',
  'origin/linux/macos-build',
  'origin/linux/runtime-fixes'
)
foreach ($branch in $branches) {
  git log --oneline "origin/main..$branch"
}
```

Expected: output identifies commits absent from `main`; no branch is deleted at this stage.

- [ ] **Step 3: Create an isolated integration worktree/branch**

Run:

```powershell
$ErrorActionPreference = 'Stop'
Set-Location 'C:\Users\Алексей\.minimax-agent\projects\singbox-client'
git worktree add '..\singbox-client-main-integration' -b maintenance/security-release-consolidation origin/main
```

Expected: a clean worktree at `..\singbox-client-main-integration`, based on `origin/main`.

- [ ] **Step 4: Integrate only missing confirmed commits in dependency order**

Use `git merge --no-ff` for a branch whose whole tip is required, or `git cherry-pick <sha>` for selected commits. Before every choice, inspect `git diff origin/main...<branch>` and retain only production-relevant fixes. Resolve conflicts by preserving newest platform behavior and the security requirements in this plan.

- [ ] **Step 5: Verify branch reachability before any remote mutation**

Run:

```powershell
$ErrorActionPreference = 'Stop'
$branches = @(
  'origin/android/v1.2.0-vpn-release',
  'origin/linux/fix-singbox-auto-update-platform',
  'origin/linux/macos-build',
  'origin/linux/runtime-fixes'
)
foreach ($branch in $branches) {
  git merge-base --is-ancestor $branch HEAD
  if ($LASTEXITCODE -ne 0) { Write-Warning "$branch is not fully contained; document intentional exclusions." }
}
```

Expected: all required retained commits are ancestors, or intentional exclusions are recorded in the final integration commit message/body.

- [ ] **Step 6: Commit only the implementation plan if not already committed**

```powershell
$ErrorActionPreference = 'Stop'
git add -- 'docs/superpowers/plans/2026-08-16-cloakwire-security-release-consolidation.md'
git commit -m 'docs: plan security and release consolidation'
```

Expected: plan is tracked independently from product changes.

## Task 2: Make app-shell update installation backend-authoritative and signature-verified

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/app_update.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src/lib/api.ts`
- Modify: `src/components/UpdateCard.tsx`
- Test: `src-tauri/src/app_update.rs` inline `#[cfg(test)]` module

**Interfaces:**
- Consumes: `UPDATER_MANIFEST_URL`, configured updater public key, `PlatformEntry { url, signature }`.
- Produces: `install_app_update(app: AppHandle, expected_version: Option<String>) -> AppResult<()>`; frontend invokes it without a URL.

- [ ] **Step 1: Write failing Rust tests for artifact origin and manifest binding**

Add tests that assert:

```rust
#[test]
fn rejects_non_https_update_asset() {
    assert!(validate_release_asset_url("http://github.com/org/repo/file.exe").is_err());
}

#[test]
fn rejects_untrusted_update_asset_host() {
    assert!(validate_release_asset_url("https://attacker.example/payload.exe").is_err());
}

#[test]
fn rejects_empty_or_invalid_manifest_signature() {
    assert!(decode_manifest_signature("").is_err());
    assert!(decode_manifest_signature("not-base64").is_err());
}
```

- [ ] **Step 2: Run the focused tests to demonstrate missing validation**

Run:

```powershell
$ErrorActionPreference = 'Stop'
Set-Location 'src-tauri'
cargo test app_update -- --nocapture
```

Expected: FAIL until helpers are implemented, or compilation fails because helpers do not exist.

- [ ] **Step 3: Add a minimal minisign verification implementation**

Add the smallest audited dependency compatible with the existing Tauri updater signature format. Implement helpers that:

```rust
fn validate_release_asset_url(raw: &str) -> AppResult<url::Url>;
fn decode_manifest_signature(encoded: &str) -> AppResult<String>;
fn verify_update_signature(public_key: &str, artifact: &[u8], signature_text: &str) -> AppResult<()>;
```

Rules: require HTTPS; permit only `github.com` and `release-assets.githubusercontent.com` final URLs for the project release path; reject any redirect that resolves elsewhere; verify bytes before writing a file.

- [ ] **Step 4: Bind install to a freshly fetched manifest entry**

Replace `install_app_update(app, download_url)` with `install_app_update(app, expected_version)`. Fetch manifest in Rust, require `available`, select current platform entry, reject a mismatched `expected_version`, validate URL and signature, download with a redirect policy that validates the final URL, verify signature, then write the verified bytes to a temporary file.

- [ ] **Step 5: Narrow the IPC and UI contract**

Change `commands.rs`, `src/lib/api.ts`, and `UpdateCard.tsx` so install calls are shaped as:

```ts
await api.installAppUpdate(appUpdate.version);
```

The UI must not store or submit `downloadUrl`; retain only version and notes for display.

- [ ] **Step 6: Run tests and frontend compilation**

Run:

```powershell
$ErrorActionPreference = 'Stop'
Set-Location 'src-tauri'
cargo test app_update -- --nocapture
Set-Location '..'
npm run build
```

Expected: focused Rust tests and frontend production build pass.

- [ ] **Step 7: Commit the security boundary change**

```powershell
$ErrorActionPreference = 'Stop'
git add -- src-tauri/Cargo.toml src-tauri/src/app_update.rs src-tauri/src/commands.rs src/lib/api.ts src/components/UpdateCard.tsx
git commit -m 'fix(updater): verify signed app artifacts in backend'
```

## Task 3: Make sing-box core updates trusted, staged, and atomic

**Files:**
- Modify: `src-tauri/src/updates.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src/lib/api.ts`
- Modify: `src/components/UpdateCard.tsx`
- Test: inline `#[cfg(test)]` module in `src-tauri/src/updates.rs`

**Interfaces:**
- Consumes: `RELEASES_LATEST_URL`, GitHub release assets, selected target triple.
- Produces: `apply_singbox_update(app: AppHandle, expected_version: Option<String>) -> AppResult<String>`; successful replacement preserves the previous runtime binary until candidate validation completes.

- [ ] **Step 1: Write failing tests for safe asset selection and checksum pairing**

Add tests that cover a mocked release with a matching archive, checksum manifest, and unrelated asset:

```rust
#[test]
fn selects_only_exact_platform_archive() { /* amd64 Windows target chooses matching zip */ }

#[test]
fn rejects_release_without_checksum_for_selected_archive() { /* error, no install */ }

#[test]
fn rejects_checksum_mismatch() { /* error, no replacement */ }
```

- [ ] **Step 2: Run focused tests before implementation**

Run:

```powershell
$ErrorActionPreference = 'Stop'
Set-Location 'src-tauri'
cargo test updates -- --nocapture
```

Expected: tests fail or helpers are missing.

- [ ] **Step 3: Refetch and bind release metadata in backend**

Remove `download_url` from public installation API. Fetch release metadata again during install, require exact expected version when supplied, select only the current OS/architecture archive, and check the final redirect URL against GitHub release/download hosts.

- [ ] **Step 4: Verify archive integrity and stage candidate**

Parse the release checksum asset using a strict `SHA256  filename` mapping. Calculate SHA-256 for downloaded archive and require equality for the selected file. Extract only into a fresh per-update staging directory. Validate candidate `sing-box version` before replacement.

- [ ] **Step 5: Atomically retain rollback safety**

Write candidate to a sibling temporary file under runtime directory, fsync where supported, rename old runtime binary to `.previous`, rename candidate to final path, verify executable version, then remove `.previous`. If verification fails, restore `.previous` and return a clear error.

- [ ] **Step 6: Update TypeScript calls and UI state**

Remove `downloadUrl` from `SingboxUpdateInfo` install state. The download UI keeps version/size display and invokes:

```ts
await api.applySingboxUpdate(sbUpdate.latest);
```

- [ ] **Step 7: Run focused and frontend checks**

Run:

```powershell
$ErrorActionPreference = 'Stop'
Set-Location 'src-tauri'
cargo test updates -- --nocapture
Set-Location '..'
npm run build
```

Expected: all targeted tests and production build pass.

- [ ] **Step 8: Commit the sing-box trust change**

```powershell
$ErrorActionPreference = 'Stop'
git add -- src-tauri/src/updates.rs src-tauri/src/commands.rs src/lib/api.ts src/components/UpdateCard.tsx
git commit -m 'fix(updater): verify and atomically install sing-box core'
```

## Task 4: Correct process reset and platform installer execution

**Files:**
- Modify: `src-tauri/src/process.rs`
- Modify: `src-tauri/src/app_update.rs`
- Test: inline tests in the modified Rust modules

**Interfaces:**
- Consumes: platform proxy abstraction and verified installer path plus explicit `InstallerKind`.
- Produces: `reset()` cleans proxy before dropping state; `spawn_installer_and_exit()` dispatches by explicit installer kind and reports failures.

- [ ] **Step 1: Write regression tests for reset cleanup and package classification**

Introduce a testable proxy cleanup function/trait and add tests:

```rust
#[tokio::test]
async fn reset_clears_proxy_before_forgetting_process_state() { /* assert cleanup call order */ }

#[test]
fn package_kind_is_derived_from_validated_asset_name() {
    assert_eq!(installer_kind_from_asset_name("Cloakwire_1.2.1_amd64.deb").unwrap(), InstallerKind::Deb);
    assert_eq!(installer_kind_from_asset_name("Cloakwire_1.2.1_amd64.AppImage").unwrap(), InstallerKind::AppImage);
}
```

- [ ] **Step 2: Run the regression tests before implementation**

Run:

```powershell
$ErrorActionPreference = 'Stop'
Set-Location 'src-tauri'
cargo test reset_clears_proxy -- --nocapture
cargo test package_kind_is_derived -- --nocapture
```

Expected: FAIL until cleanup/order and explicit kind code exist.

- [ ] **Step 3: Clear system proxy during reset**

Call `clear_system_proxy()` before discarding `child`, config, and controller state. Log a recoverable cleanup error; do not skip traffic shutdown. Keep path idempotent across TUN-only and stopped sessions.

- [ ] **Step 4: Preserve asset kind and use it on Linux**

Derive `InstallerKind` from the manifest asset name after URL validation. Preserve the matching extension in the temporary file name. Pass `InstallerKind` into launcher so `.deb` always uses `pkexec dpkg -i` and AppImage always gets executable permissions and direct launch.

- [ ] **Step 5: Replace fire-and-forget macOS install with an acknowledged helper path**

Create a helper command invocation that receives the verified DMG path, mounts it, validates `Cloakwire.app`, copies to a temporary `/Applications` sibling, replaces the old app, and detaches. Check every command status. Main app exits only after helper spawn succeeds; return the helper’s startup error to UI. Do not discard `hdiutil`/copy errors.

- [ ] **Step 6: Run focused regression suite**

Run:

```powershell
$ErrorActionPreference = 'Stop'
Set-Location 'src-tauri'
cargo test reset_clears_proxy -- --nocapture
cargo test package_kind_is_derived -- --nocapture
cargo test app_update -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit lifecycle and installer changes**

```powershell
$ErrorActionPreference = 'Stop'
git add -- src-tauri/src/process.rs src-tauri/src/app_update.rs
git commit -m 'fix(runtime): clean proxy and dispatch verified installers'
```

## Task 5: Fix mobile Auto selection and add a frontend regression harness

**Files:**
- Modify: `package.json`
- Modify: `src/mobile/MobileApp.tsx`
- Modify: `src/mobile/screens/HomeScreen.tsx`
- Create: `src/mobile/MobileApp.test.tsx` (or project-established equivalent)

**Interfaces:**
- Consumes: `GeneratorSettings.default_outbound: string | null` and flattened `Outbound[]`.
- Produces: `selectedIndex = -1` for Auto; a selected profile index only when tag matches a current supported profile.

- [ ] **Step 1: Add a minimal test runner only if none exists**

Use Vitest and Testing Library if absent from project. Add scripts:

```json
"test": "vitest run",
"test:watch": "vitest"
```

Configure jsdom and existing `@/` alias without altering production Vite behavior.

- [ ] **Step 2: Write the failing Auto-state test**

Test initial settings with two profiles and `default_outbound: null`; assert that the home UI exposes `Auto (best latency)` and does not render the first profile as the active outbound. Add a second case where `default_outbound` equals the second profile tag and that profile is selected.

- [ ] **Step 3: Run test to verify failure**

Run:

```powershell
$ErrorActionPreference = 'Stop'
npm test -- MobileApp
```

Expected: FAIL because initial state is hardcoded to `0` and clamp effect turns `-1` into `0`.

- [ ] **Step 4: Derive and preserve selection correctly**

Add a small pure helper:

```ts
export function selectedProfileIndex(profiles: Outbound[], selectedTag: string | null): number
```

Return `-1` when tag is null/missing/unsupported. Use it to initialize and reconcile selection after profile changes. In `HomeScreen`, render explicit Auto label/details for `selectedIndex === -1`.

- [ ] **Step 5: Run test and production build**

Run:

```powershell
$ErrorActionPreference = 'Stop'
npm test -- MobileApp
npm run build
```

Expected: PASS.

- [ ] **Step 6: Commit mobile correctness change**

```powershell
$ErrorActionPreference = 'Stop'
git add -- package.json package-lock.json src/mobile/MobileApp.tsx src/mobile/screens/HomeScreen.tsx src/mobile/MobileApp.test.tsx
if (Test-Path 'vitest.config.ts') { git add -- vitest.config.ts }
git commit -m 'fix(mobile): show Auto when no outbound is pinned'
```

## Task 6: Harden release scripts and make updater assets reproducible

**Files:**
- Modify: `scripts/release.ps1`
- Modify: `scripts/write-latest-json.ps1`
- Create: `scripts/validate-release.ps1`
- Test: PowerShell dry-run using a disposable fixture directory

**Interfaces:**
- Consumes: `-Version`, `-DistPath`, expected target list, signed `.sig` sidecars.
- Produces: UTF-8-no-BOM `latest.json` only after required artifacts/signatures/version checks pass.

- [ ] **Step 1: Write fixture-driven validation checks before changing release flow**

Create a temporary directory with an intentionally stale `Cloakwire_1.2.0_x64-setup.exe` and request `-Version 1.2.1`. Run validator expecting non-zero exit with an explicit version mismatch. Create a second fixture missing a `.sig` file and expect signature error.

- [ ] **Step 2: Make release build failure terminal**

After `npm run tauri:build`, capture `$LASTEXITCODE` outside the output pipeline and terminate when it is non-zero:

```powershell
npm run tauri:build 2>&1 | Select-Object -Last 10
$buildExit = $LASTEXITCODE
if ($buildExit -ne 0) { throw "tauri build failed with exit code $buildExit" }
```

- [ ] **Step 3: Isolate and validate artifacts**

Have `release.ps1` use a fresh versioned staging directory. Require exact filenames containing `$Version`, exactly one updater artifact per target, and a matching `.sig` for every manifest artifact before writing manifest.

- [ ] **Step 4: Parameterize manifest generation**

Replace hardcoded `1.0.7` and mixed artifact versions in `write-latest-json.ps1` with mandatory `-Version`, `-DistPath`, `-BaseUrl`, and `-RequiredPlatforms` parameters. Preserve UTF-8 without BOM. Fail if any required platform asset or signature is absent.

- [ ] **Step 5: Implement reusable validation script**

`validate-release.ps1` accepts manifest path, dist path, version, and expected platforms. It parses JSON, requires every target entry, verifies `https` URL, validates filename version, and requires corresponding staged artifact/signature.

- [ ] **Step 6: Run fixtures and syntax checks**

Run:

```powershell
$ErrorActionPreference = 'Stop'
pwsh -NoProfile -File scripts\validate-release.ps1 -ManifestPath .\test-fixtures\latest.json -DistPath .\test-fixtures -Version 1.2.1 -RequiredPlatforms windows-x86_64
$null = [scriptblock]::Create((Get-Content -Raw -Encoding UTF8 scripts\release.ps1))
$null = [scriptblock]::Create((Get-Content -Raw -Encoding UTF8 scripts\write-latest-json.ps1))
```

Expected: valid fixture passes; stale/missing fixtures fail with a descriptive error.

- [ ] **Step 7: Commit release-script safeguards**

```powershell
$ErrorActionPreference = 'Stop'
git add -- scripts/release.ps1 scripts/write-latest-json.ps1 scripts/validate-release.ps1
git commit -m 'fix(release): reject failed builds and stale artifacts'
```

## Task 7: Produce signed macOS updater archives and pin CI inputs

**Files:**
- Modify: `.github/workflows/release-macos.yml`
- Modify: `.github/workflows/release-android.yml`
- Create: `.github/workflows/release-validation.yml`
- Modify: `scripts/validate-release.ps1`

**Interfaces:**
- Consumes: architecture matrix, updater signing secret/key provisioning, packaged `.app`, DMG, and staged release artifacts.
- Produces: one DMG and one signed `.app.tar.gz` per Darwin architecture; validation reports release artifact coverage.

- [ ] **Step 1: Add a failing static assertion for missing Darwin updater archive**

Add release validation input fixture with only DMGs. Require `darwin-aarch64` and `darwin-x86_64`; expect validation to fail because `.app.tar.gz` and `.sig` are absent.

- [ ] **Step 2: Package and sign Darwin updater artifacts**

After app/dmg build, find the produced `.app`, create architecture-specific `Cloakwire_<version>_<arch>.app.tar.gz`, sign it with the existing updater signing mechanism, retain DMG separately, and upload both artifacts plus signature.

- [ ] **Step 3: Assemble a complete staged release set**

In the collector job, download all architecture artifacts into a deterministic `dist/` shape. Invoke `scripts/write-latest-json.ps1` with both Darwin platforms and all other release targets intended by that workflow. Invoke `scripts/validate-release.ps1` before upload.

- [ ] **Step 4: Pin external actions and sing-box input revisions**

Replace action tag references used by release workflows with documented immutable commit SHAs. Replace mutable `sing-box-lx` branch with a reviewed commit SHA exposed as a single workflow env variable. Emit that SHA into artifact metadata/log output.

- [ ] **Step 5: Add independent release-validation workflow**

Create a `workflow_dispatch`/reusable workflow accepting staged artifacts and expected platforms. It calls `validate-release.ps1`, publishes its report, and fails before any release upload path can proceed on missing signature/platform/version mismatch.

- [ ] **Step 6: Validate workflow syntax and shell paths**

Run a YAML parser available in the repository toolchain or `npx prettier --check` only if configured. At minimum, inspect every PowerShell invocation under Windows and every bash invocation under macOS/Ubuntu for correct file paths, required environment variables, and artifact names.

- [ ] **Step 7: Commit CI and macOS release changes**

```powershell
$ErrorActionPreference = 'Stop'
git add -- .github/workflows/release-macos.yml .github/workflows/release-android.yml .github/workflows/release-validation.yml scripts/validate-release.ps1
git commit -m 'ci(release): publish validated macos updater assets'
```

## Task 8: Run verification, integrate into main, push, and retire feature branches

**Files:**
- Modify: project files from Tasks 2–7 only
- Modify: Git refs on GitHub only after checks pass

**Interfaces:**
- Consumes: completed implementation commits and verified local integration worktree.
- Produces: updated `origin/main` and removal of four obsolete remote feature refs.

- [ ] **Step 1: Run repository hygiene checks**

Run:

```powershell
$ErrorActionPreference = 'Stop'
git status --short
git diff --check
git ls-files -o --exclude-standard
```

Expected: only deliberate source, docs, and workflow changes are staged/committed; generated `target/`, `.mobile-ui-check/`, and `tsconfig.tsbuildinfo` are absent from commit candidates.

- [ ] **Step 2: Run full available checks**

Run:

```powershell
$ErrorActionPreference = 'Stop'
npm run build
npm test
Set-Location src-tauri
cargo check
cargo test
```

Expected: all checks pass. If Cargo is still unavailable, record it as an environment blocker and run all non-Rust checks; do not claim Rust compilation passed.

- [ ] **Step 3: Independently review integration diff**

Use a verifier to inspect `origin/main...HEAD` for concrete regressions in update trust boundaries, release scripts, and platform dispatch. Address verified findings with dedicated commits before publishing.

- [ ] **Step 4: Verify intended branch inclusion**

Run:

```powershell
$ErrorActionPreference = 'Stop'
$branches = @(
  'origin/android/v1.2.0-vpn-release',
  'origin/linux/fix-singbox-auto-update-platform',
  'origin/linux/macos-build',
  'origin/linux/runtime-fixes'
)
foreach ($branch in $branches) {
  git log --oneline "HEAD..$branch"
}
```

Expected: no required production commits are left only on a feature branch. Document intentional rejected commits before deletion.

- [ ] **Step 5: Push integration branch to `main`**

Run only after Steps 1–4 pass:

```powershell
$ErrorActionPreference = 'Stop'
git push origin HEAD:main
git ls-remote --heads origin main
```

Expected: remote `main` resolves to the verified integration commit.

- [ ] **Step 6: Delete remote feature branches**

Run only after confirming main includes/replaces each branch’s required work:

```powershell
$ErrorActionPreference = 'Stop'
$branches = @(
  'android/v1.2.0-vpn-release',
  'linux/fix-singbox-auto-update-platform',
  'linux/macos-build',
  'linux/runtime-fixes'
)
foreach ($branch in $branches) { git push origin --delete $branch }
git fetch --prune origin
git branch -r
```

Expected: `origin/main` is the only remote branch, while existing tags remain unchanged.

- [ ] **Step 7: Remove local feature worktrees/branches only after remote verification**

Run:

```powershell
$ErrorActionPreference = 'Stop'
git worktree list
# Remove only worktrees no longer in use, then delete merged local branches.
git branch -vv
```

Expected: local cleanup does not destroy unpushed or unmerged work.

- [ ] **Step 8: Report exact verification and remaining constraints**

Report: final `main` commit, deleted remote branch names, validation commands/results, release artifacts deliberately not published, and any blocked check such as missing local Cargo.
