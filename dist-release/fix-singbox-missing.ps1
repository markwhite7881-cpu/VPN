# Cloakwire: восстановление sing-box.exe рядом с cloakwire.exe
# Загрузка выполняется только с официальных HTTPS GitHub API/release endpoints.
$ErrorActionPreference = 'Stop'

$SingboxRepository = 'SagerNet/sing-box'
$GitHubApiBase = "https://api.github.com/repos/$SingboxRepository"

function Write-Section { param([string]$Title); Write-Host "`n=== $Title ===" -ForegroundColor Cyan }

function Find-CloakwireInstall {
    $candidates = @(
        (Join-Path $env:LOCALAPPDATA 'Cloakwire'),
        'C:\Program Files\Cloakwire',
        'C:\Program Files (x86)\Cloakwire'
    )
    foreach ($dir in $candidates) {
        if (Test-Path (Join-Path $dir 'cloakwire.exe')) { return $dir }
    }
    return $null
}

function Get-GitHubJson {
    param([string]$Uri)
    if ($Uri -notmatch '^https://api\.github\.com/repos/SagerNet/sing-box/') {
        throw "Refusing non-official GitHub API URL: $Uri"
    }
    return Invoke-RestMethod -Uri $Uri -Headers @{ 'User-Agent' = 'Cloakwire-recovery' } -UseBasicParsing
}

function Get-OfficialAssetText {
    param([string]$Uri)
    if ($Uri -notmatch '^https://github\.com/SagerNet/sing-box/releases/download/') {
        throw "Refusing non-official GitHub release URL: $Uri"
    }
    return (Invoke-WebRequest -Uri $Uri -Headers @{ 'User-Agent' = 'Cloakwire-recovery' } -UseBasicParsing).Content
}

function Get-AssetDownload {
    param([object]$Asset, [string]$Destination)
    if ($Asset.browser_download_url -notmatch '^https://github\.com/SagerNet/sing-box/releases/download/') {
        throw "Release asset URL is not an official HTTPS GitHub URL"
    }
    Invoke-WebRequest -Uri $Asset.browser_download_url -OutFile $Destination -Headers @{ 'User-Agent' = 'Cloakwire-recovery' } -UseBasicParsing
}

function Get-ChecksumForAsset {
    param([string]$Text, [string]$AssetName)
    foreach ($line in ($Text -split "`r?`n")) {
        if ($line -match '^\s*([0-9A-Fa-f]{64})\s+\*?(.+?)\s*$' -and $Matches[2] -eq $AssetName) {
            return $Matches[1].ToLowerInvariant()
        }
    }
    throw "Official checksum file has no SHA-256 entry for $AssetName"
}

function Get-ExpectedExecutable {
    param([string]$Root)
    $matches = @(Get-ChildItem -Path $Root -Recurse -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -eq 'sing-box-windows-amd64.exe' -or $_.Name -eq 'sing-box.exe' })
    if ($matches.Count -ne 1) { throw "Archive must contain exactly one expected Windows executable" }
    return $matches[0]
}

Write-Section 'Cloakwire: проверка sing-box.exe'
$installDir = Find-CloakwireInstall
if (-not $installDir) { Write-Host 'Не нашёл установленный Cloakwire.' -ForegroundColor Red; exit 1 }
$singboxPath = Join-Path $installDir 'sing-box.exe'
$singboxLongPath = Join-Path $installDir 'sing-box-x86_64-pc-windows-msvc.exe'
if ((Test-Path $singboxPath) -or (Test-Path $singboxLongPath)) { Write-Host 'OK: sing-box уже на месте' -ForegroundColor Green; exit 0 }

$tempRoot = Join-Path $env:TEMP ("cloakwire-singbox-recovery-" + [guid]::NewGuid().ToString('N'))
$zip = Join-Path $tempRoot 'sing-box.zip'
$extract = Join-Path $tempRoot 'extract'
New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
try {
    $release = Get-GitHubJson "$GitHubApiBase/releases/latest"
    if ($release.tag_name -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+$') { throw "Unexpected official release tag: $($release.tag_name)" }
    $asset = @($release.assets | Where-Object { $_.name -match '^sing-box-[0-9]+\.[0-9]+\.[0-9]+-windows-amd64\.zip$' })
    $checksumAsset = @($release.assets | Where-Object { $_.name -in @('SHA256SUMS.txt', 'sha256sums.txt', 'checksums.txt') })
    if ($asset.Count -ne 1 -or $checksumAsset.Count -gt 1) { throw 'Official release must provide one Windows amd64 archive and at most one checksum asset' }
    $assetName = $asset[0].name
    if ($checksumAsset.Count -eq 1) {
        $expectedHash = Get-ChecksumForAsset (Get-OfficialAssetText $checksumAsset[0].browser_download_url) $assetName
    } elseif ($asset[0].digest -match '^sha256:([0-9A-Fa-f]{64})$') {
        $expectedHash = $Matches[1].ToLowerInvariant()
    } else {
        throw "Official release metadata has no SHA-256 checksum for $assetName"
    }
    Get-AssetDownload $asset[0] $zip
    $actualHash = (Get-FileHash -Path $zip -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $expectedHash) { throw "SHA-256 mismatch for $assetName (expected $expectedHash, got $actualHash)" }
    Expand-Archive -Path $zip -DestinationPath $extract -Force
    $extracted = Get-ExpectedExecutable $extract
    $versionOutput = (& $extracted.FullName version 2>&1 | Out-String)
    $releaseVersion = $release.tag_name.Substring(1)
    if ($versionOutput -notmatch [regex]::Escape($releaseVersion)) { throw "Downloaded executable version does not match official release $($release.tag_name)" }
    Copy-Item -LiteralPath $extracted.FullName -Destination $singboxPath -Force
    Write-Host "Скопировал проверенный $assetName ($actualHash) в: $singboxPath" -ForegroundColor Green
} catch {
    Write-Host "Восстановление остановлено: $($_.Exception.Message)" -ForegroundColor Red
    Write-Host 'Установленный бинарник не изменён.' -ForegroundColor Yellow
    exit 1
} finally {
    if (Test-Path $tempRoot) { Remove-Item $tempRoot -Recurse -Force -ErrorAction SilentlyContinue }
}

Write-Section 'Следующий шаг: исключение в антивирусе'
Write-Host "Если sing-box.exe снова пропадает, добавьте папку $installDir в исключения антивируса."
Write-Host 'Готово. Запусти Cloakwire заново.' -ForegroundColor Green
