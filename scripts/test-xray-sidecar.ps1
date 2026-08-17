$ErrorActionPreference = 'Stop'
$ProjectRoot = Split-Path -Parent $PSScriptRoot
$metadata = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'xray-core.json') -Raw -Encoding UTF8 | ConvertFrom-Json
if ($metadata.version -notmatch '^v\d+\.\d+\.\d+$') { throw 'Invalid Xray version metadata' }
if ($metadata.archiveUrl -notmatch '^https://github\.com/XTLS/Xray-core/releases/download/') { throw 'Xray archive URL is not an official HTTPS XTLS release URL' }
foreach ($field in @('archiveSha256', 'executableSha256')) {
    if (([string]$metadata.$field) -notmatch '^[0-9a-f]{64}$') { throw "Invalid $field metadata" }
}
if ([int64]$metadata.executableSize -le 0) { throw 'Xray executable size must be positive' }
if ([string]$metadata.archiveMember -cne 'xray.exe') { throw 'Unexpected Xray archive member' }
$config = Get-Content -LiteralPath (Join-Path $ProjectRoot 'src-tauri\tauri.conf.json') -Raw -Encoding UTF8 | ConvertFrom-Json
if (@($config.bundle.externalBin) -notcontains 'binaries/xray') { throw 'Tauri bundle does not declare binaries/xray' }
$sidecar = Join-Path $ProjectRoot 'src-tauri\binaries\xray-x86_64-pc-windows-msvc.exe'
if (Test-Path -LiteralPath $sidecar -PathType Leaf) {
    $file = Get-Item -LiteralPath $sidecar
    if ($file.Length -ne [int64]$metadata.executableSize) { throw 'Prepared Xray sidecar has wrong size' }
    $hash = (Get-FileHash -LiteralPath $sidecar -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -cne $metadata.executableSha256) { throw 'Prepared Xray sidecar has wrong SHA-256' }
    $version = (& $sidecar version 2>&1 | Out-String).Trim()
    $expectedVersion = $metadata.version.TrimStart('v')
    if ($version -notmatch [regex]::Escape($expectedVersion)) { throw 'Prepared Xray sidecar has wrong version' }
    Write-Host 'Xray sidecar metadata and binary: PASS'
} else {
    Write-Host 'Xray metadata and Tauri sidecar declaration: PASS (binary not prepared locally)'
}
