$ErrorActionPreference = 'Stop'

$projectRoot = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $projectRoot 'src-tauri\Cargo.toml'
$manifest = [IO.File]::ReadAllText($manifestPath, [Text.UTF8Encoding]::new($false))

if ($manifest -notmatch '(?m)^\[workspace\]\s*$') {
    throw 'src-tauri/Cargo.toml must declare [workspace] so Cargo does not inherit an unrelated parent Cargo.toml.'
}

$signerManifestPath = Join-Path $projectRoot 'src-tauri\crates\tauri-signer\Cargo.toml'
$signerManifest = [IO.File]::ReadAllText($signerManifestPath, [Text.UTF8Encoding]::new($false))
if ($signerManifest -notmatch '(?m)^\[workspace\]\s*$') {
    throw 'src-tauri/crates/tauri-signer/Cargo.toml must declare [workspace] so release tooling does not inherit an unrelated parent Cargo.toml.'
}

Write-Output 'PASS: application and standalone signer declare explicit Cargo workspace boundaries.'
