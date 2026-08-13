# Release helper for the Tauri updater.
#
# What this does:
#   1. `npm run tauri build` — produces MSI + NSIS in
#      src-tauri\target\release\bundle\
#   2. `src-tauri\crates\tauri-signer\target\release\tauri-signer.exe`
#      — signs each installer with the project's private key
#      (src-tauri\.tauri-updater.key, NEVER commit this) and emits
#      `.sig` sidecar files. We use our own signer because
#      `npx tauri signer sign` hangs on Windows after
#      "Signing without password." (TTY-detection bug in
#      tauri-cli 2.x).
#   3. Produces `latest.json` — the manifest the running app
#      fetches from GitHub Releases to know there's a new version.
#   4. Prints the `gh release create` command you'll need to
#      upload the artifacts + latest.json to GitHub.
#
# Usage (PowerShell):
#   .\scripts\release.ps1 -Version 1.0.1
#
# The script does NOT push to GitHub — it stops just short of
# that so you can review the manifest, then asks you to confirm.

param(
    [Parameter(Mandatory = $true)]
    [string]$Version
)

$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
$BundleRoot = Join-Path $ProjectRoot "src-tauri\target\release\bundle"
$KeyPath = Join-Path $ProjectRoot "src-tauri\.tauri-updater.key"
$LatestPath = Join-Path $ProjectRoot "latest.json"
$SignerExe = Join-Path $ProjectRoot "src-tauri\crates\tauri-signer\target\release\tauri-signer.exe"

if (-not (Test-Path $SignerExe)) {
    Write-Host "ERROR: tauri-signer.exe not found at $SignerExe" -ForegroundColor Red
    Write-Host "Build it with: cd src-tauri/crates/tauri-signer && cargo build --release"
    exit 1
}

if (-not (Test-Path $KeyPath)) {
    Write-Host "ERROR: signing key not found at $KeyPath" -ForegroundColor Red
    Write-Host "Generate one with: npx tauri signer generate -w $KeyPath"
    exit 1
}

# 1) Build installers. This is the long step (~3 min).
# Make sure cargo / rustc are on PATH for the subprocess — tauri
# shells out to `cargo metadata` and friends. We pull the path
# from the standard install location if it's not already there.
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
    if (Test-Path $cargoBin) {
        $env:PATH = "$cargoBin;$env:PATH"
    }
}

Write-Host "==> Building installers (this takes ~3 min)..." -ForegroundColor Cyan
Push-Location $ProjectRoot
try {
    npm run tauri:build 2>&1 | Select-Object -Last 10
} finally {
    Pop-Location
}

$msiDir = Join-Path $BundleRoot "msi"
$nsisDir = Join-Path $BundleRoot "nsis"

if (-not (Test-Path $msiDir) -and -not (Test-Path $nsisDir)) {
    Write-Host "ERROR: no bundle output at $BundleRoot" -ForegroundColor Red
    exit 1
}

# 2) Sign every installer and collect {url, signature} pairs.
# Tauri 2 manifest format: { version, notes, pub_date, platforms: { "windows-x86_64": { url, signature } } }.
$signatures = @{}
$artifacts = @()

$items = @()
if (Test-Path $msiDir)  { $items += Get-ChildItem $msiDir  -Filter "*.msi" }
if (Test-Path $nsisDir) { $items += Get-ChildItem $nsisDir -Filter "*.exe" }

foreach ($item in $items) {
    $filePath = $item.FullName
    Write-Host "    signing $filePath..." -ForegroundColor DarkCyan
    # Our local tauri-signer writes `<file>.sig` next to the file
    # in the standard minisign format (no password prompt — the
    # tauri-generated key file uses KDF with empty passphrase, and
    # the tauri-signer binary passes `Some("")` accordingly).
    & $SignerExe -k $KeyPath $filePath 2>&1 | Select-Object -Last 3
    $sigPath = "$filePath.sig"
    if (-not (Test-Path $sigPath)) {
        Write-Host "ERROR: signature file not produced for $filePath" -ForegroundColor Red
        exit 1
    }
    # Standard minisign signature file: 4 lines, alternating
    # comment / payload. Line 1 is the untrusted comment, line 2
    # is the Ed25519 signature, line 3 is the trusted comment,
    # line 4 is the trusted-payload signature. The Rust verifier
    # wants only line 2.
    $sigLines = (Get-Content $sigPath -Encoding UTF8)
    $sigB64 = ($sigLines | Select-Object -Skip 1 -First 1).Trim()
    $fileName = [System.IO.Path]::GetFileName($filePath)
    $url = "https://github.com/markwhite7881-cpu/cloakwire/releases/download/v$Version/$fileName"
    $signatures["windows-x86_64"] = @{ url = $url; signature = $sigB64 }
    $artifacts += $filePath
}

# 3) Compose latest.json. Use a here-string + simple replacement to
# avoid the JSON-building-in-PowerShell traps (ConvertTo-Json adds
# a BOM on PS 5.1, and string interpolation is finicky).
$platformsJson = ($signatures.GetEnumerator() | ForEach-Object {
    $key = $_.Key
    $obj = $_.Value
    # `ConvertTo-Json -Compress` on a single object is fine.
    $inner = $obj | ConvertTo-Json -Compress
    "`"$key`": $inner"
}) -join ", "

$pubDate = (Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ")
$manifest = @"
{
  "version": "$Version",
  "notes": "Release v$Version — see the GitHub release notes for the full changelog.",
  "pub_date": "$pubDate",
  "platforms": {
    $platformsJson
  }
}
"@

# Write WITHOUT BOM. PS 5.1's Out-File adds UTF-8 BOM by default
# which Tauri's manifest parser can't handle.
[IO.File]::WriteAllText(
    $LatestPath,
    $manifest,
    [Text.UTF8Encoding]::new($false)
)

Write-Host ""
Write-Host "==> Done." -ForegroundColor Green
Write-Host "    Manifest: $LatestPath" -ForegroundColor Green
Write-Host "    Artifacts:" -ForegroundColor Green
$artifacts | ForEach-Object { Write-Host "      $_" -ForegroundColor Green }
Write-Host ""

# 4) Print the gh release create command. The user runs it
# manually after reviewing the manifest.
Write-Host "==> Next step: create the GitHub release manually:" -ForegroundColor Yellow
Write-Host ""
$artifactList = ($artifacts + @($LatestPath)) -join '" "'
Write-Host "  gh release create v$Version \"" -NoNewline
Write-Host $artifactList -NoNewline
Write-Host "\" --title \"v$Version\" --generate-notes" -ForegroundColor Yellow
Write-Host ""
Write-Host "==> DO NOT publish until you've reviewed latest.json." -ForegroundColor Magenta
