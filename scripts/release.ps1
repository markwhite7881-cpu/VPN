# Release helper for the Tauri updater.
#
# What this does:
#   1. `npm run tauri build` — produces MSI + NSIS + portable .exe
#      in src-tauri\target\release\bundle\
#   2. `npx tauri signer sign` — signs each installer with the
#      project's private key (src-tauri\.tauri-updater.key, NEVER
#      commit this) and emits `.sig` sidecar files.
#   3. Produces `latest.json` — the manifest the running app
#      fetches from GitHub Releases to know there's a new version.
#   4. Prints the `gh release create` command you'll need to
#      upload the artifacts + latest.json to GitHub.
#
# Usage (PowerShell):
#   .\scripts\release.ps1 -Version 0.3.1
#
# The script does NOT push to GitHub — it stops just short of
# that so you can review the manifest, then asks you to confirm
# before doing anything irreversible.
#
# Signing key: see src-tauri/.tauri-updater.key (gitignored).
# Public counterpart (committed): src-tauri/.tauri-updater.key.pub
# — embedded into the binary via tauri.conf.json > plugins.updater.pubkey.

param(
    [Parameter(Mandatory = $true)]
    [string]$Version
)

$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
$BundleRoot = Join-Path $ProjectRoot "src-tauri\target\release\bundle"
$KeyPath = Join-Path $ProjectRoot "src-tauri\.tauri-updater.key"

if (-not (Test-Path $KeyPath)) {
    Write-Host "ERROR: signing key not found at $KeyPath" -ForegroundColor Red
    Write-Host "Generate one with: npx tauri signer generate -w $KeyPath"
    exit 1
}

Write-Host "==> Building installers (this takes ~3 min)..." -ForegroundColor Cyan
Push-Location $ProjectRoot
try {
    npm run tauri:build 2>&1 | Select-Object -Last 10
} finally {
    Pop-Location
}

# Tauri emits different platform dirs (msi/, nsis/, deb/, app/, dmg/).
# We only ship Windows here, so we look for msi and nsis. The macOS
# and Linux bundles are signed differently and aren't part of the
# singbox-client release yet.
$msiDir = Join-Path $BundleRoot "msi"
$nsisDir = Join-Path $BundleRoot "nsis"

if (-not (Test-Path $msiDir) -and -not (Test-Path $nsisDir)) {
    Write-Host "ERROR: no bundle output at $BundleRoot" -ForegroundColor Red
    exit 1
}

# Collect every installer we want to ship, with the right name
# for the manifest. The naming must match Tauri's expectations:
# `<name>_<version>_<arch>.<ext>`, but the manifest itself just
# needs `url` and `signature` (a base64-encoded minisign signature).
$artifacts = @()

function Sign-And-Add {
    param(
        [string]$Path,
        [string]$Platform # "windows" | "macos" | "linux"
    )
    if (-not (Test-Path $Path)) { return }
    Write-Host "    signing $Path..." -ForegroundColor DarkCyan
    # `tauri signer sign` writes `<file>.sig` next to the file.
    # We pass the private key explicitly; on CI you'd use
    # `TAURI_SIGNING_PRIVATE_KEY` env var instead.
    npx tauri signer sign --help *> $null 2>&1  # warm up node
    & npx tauri signer sign -k $KeyPath $Path 2>&1 | Select-Object -Last 3
    $sigPath = "$Path.sig"
    if (-not (Test-Path $sigPath)) {
        Write-Host "ERROR: signature file not produced for $Path" -ForegroundColor Red
        exit 1
    }
    $sig = (Get-Content $sigPath -Raw -Encoding UTF8).Trim()
    # `tauri signer sign` wraps the raw signature in "untrusted
    # comment: ..." headers. The Rust verifier only needs the last
    # 128 bytes (the actual Ed25519 signature, base64-encoded in
    # minisign's `signature:<base64>` line).
    $sigB64 = ($sig -split "`n" | Where-Object { $_.StartsWith("signature:") } | Select-Object -First 1) `
        -replace '^signature:\s*', ''
    $rel = $Path.Substring($ProjectRoot.Length).Replace('\', '/').TrimStart('/')
    $script:artifacts += [pscustomobject]@{
        url       = "https://github.com/markwhite7881-cpu/VPN/releases/download/v$Version/$([System.IO.Path]::GetFileName($Path))"
        signature = $sigB64
        path      = $Path
        rel       = $rel
    }
}

Write-Host "==> Signing installers..." -ForegroundColor Cyan
Get-ChildItem $msiDir -Filter "*.msi" -ErrorAction SilentlyContinue | ForEach-Object {
    Sign-And-Add -Path $_.FullName -Platform "windows"
}
Get-ChildItem $nsisDir -Filter "*.exe" -ErrorAction SilentlyContinue | ForEach-Object {
    Sign-And-Add -Path $_.FullName -Platform "windows"
}

# Compose the manifest. Tauri 2 expects this shape (see the
# `tauri-plugin-updater` source for the exact contract):
{
  "version": "$Version",
  "notes": "Release v$Version. See CHANGELOG.md for details.",
  "pub_date": (Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ"),
  "platforms": {
    $($artifacts | ForEach-Object {
        # `windows-x86_64` is the manifest key for the running
        # binary; Tauri looks it up by current platform + arch.
        if ($_.path -like '*.msi' -or $_.path -like '*.exe') {
            "`"windows-x86_64`": { `"url`": `"$($_.url)`", `"signature`": `"$($_.signature)`" }"
        }
    } | ConvertTo-Json -Compress)
  }
} | ConvertTo-Json -Depth 10 | Out-File -Encoding utf8 (Join-Path $ProjectRoot "latest.json")

# `ConvertTo-Json` adds a BOM on PowerShell 5.1 which Tauri
# chokes on. Strip it explicitly.
$latestPath = Join-Path $ProjectRoot "latest.json"
[IO.File]::WriteAllText(
    $latestPath,
    [IO.File]::ReadAllText($latestPath),
    [Text.UTF8Encoding]::new($false)
)

Write-Host ""
Write-Host "==> Done. Manifest written to $latestPath" -ForegroundColor Green
Write-Host "==> Artifacts:" -ForegroundColor Green
$artifacts | Format-Table -AutoSize

Write-Host ""
Write-Host "Next step: push to GitHub:" -ForegroundColor Yellow
Write-Host "  gh release create v$Version ``" -NoNewline
Write-Host "$($artifacts.path -join ' ') latest.json ``" -NoNewline
Write-Host "--title `"v$Version`" --generate-notes" -ForegroundColor Yellow
Write-Host ""
Write-Host "DO NOT publish until you've reviewed the .sig files manually." -ForegroundColor Magenta
