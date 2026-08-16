$ErrorActionPreference = 'Stop'

$projectRoot = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $projectRoot 'src-tauri\Cargo.toml'
$manifest = [IO.File]::ReadAllText($manifestPath, [Text.UTF8Encoding]::new($false))

if ($manifest -notmatch '(?m)^\[workspace\]\s*$') {
    throw 'src-tauri/Cargo.toml must declare [workspace] so Cargo does not inherit an unrelated parent Cargo.toml.'
}

Write-Output 'PASS: src-tauri is an explicit Cargo workspace root.'
