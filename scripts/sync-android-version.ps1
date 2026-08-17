param(
    [switch]$CheckOnly
)

$ErrorActionPreference = 'Stop'

$projectRoot = Split-Path -Parent $PSScriptRoot
$tauriConfigPath = Join-Path $projectRoot 'src-tauri\tauri.conf.json'
$propertiesPath = Join-Path $projectRoot 'src-tauri\gen\android\app\tauri.properties'

if (-not (Test-Path -LiteralPath $tauriConfigPath -PathType Leaf)) {
    throw "Canonical Tauri config is missing: $tauriConfigPath"
}

try {
    $tauriConfig = Get-Content -LiteralPath $tauriConfigPath -Raw -Encoding UTF8 | ConvertFrom-Json
} catch {
    throw "Canonical Tauri config is not valid JSON: $tauriConfigPath"
}

$version = [string]$tauriConfig.version
if ($version -notmatch '^(?<major>0|[1-9][0-9]*)\.(?<minor>0|[1-9][0-9]*)\.(?<patch>0|[1-9][0-9]*)$') {
    throw "Canonical Tauri version must be a numeric x.y.z semantic version; found '$version'."
}

try {
    $major = [Int64]$Matches['major']
    $minor = [Int64]$Matches['minor']
    $patch = [Int64]$Matches['patch']
} catch [OverflowException] {
    throw "Canonical Tauri version '$version' contains a numeric component outside the supported integer range."
}

$maxAndroidVersionCode = [Int64][Int32]::MaxValue
if ($major -gt [Math]::Floor($maxAndroidVersionCode / 1000000L)) {
    throw "Canonical Tauri version '$version' produces an Android versionCode outside the supported integer range."
}

$remainingVersionCode = $maxAndroidVersionCode - ($major * 1000000L)
if ($minor -gt [Math]::Floor($remainingVersionCode / 1000L)) {
    throw "Canonical Tauri version '$version' produces an Android versionCode outside the supported integer range."
}

$remainingVersionCode -= $minor * 1000L
if ($patch -gt $remainingVersionCode) {
    throw "Canonical Tauri version '$version' produces an Android versionCode outside the supported integer range."
}

$versionCode = $major * 1000000L + $minor * 1000L + $patch

$expectedContent = "tauri.android.versionName=$version`ntauri.android.versionCode=$versionCode`n"

if ($CheckOnly) {
    if (-not (Test-Path -LiteralPath $propertiesPath -PathType Leaf)) {
        throw "Android version metadata is missing: $propertiesPath"
    }

    $actualBytes = [IO.File]::ReadAllBytes($propertiesPath)
    $expectedBytes = [Text.UTF8Encoding]::new($false).GetBytes($expectedContent)
    if (-not [Linq.Enumerable]::SequenceEqual([byte[]]$actualBytes, [byte[]]$expectedBytes)) {
        throw "Android version metadata is incorrect: $propertiesPath. Run scripts\sync-android-version.ps1 to regenerate it."
    }

    Write-Output "Android version metadata is synchronized: versionName=$version; versionCode=$versionCode"
    return
}

$propertiesDirectory = Split-Path -Parent $propertiesPath
[IO.Directory]::CreateDirectory($propertiesDirectory) | Out-Null
[IO.File]::WriteAllText($propertiesPath, $expectedContent, [Text.UTF8Encoding]::new($false))
Write-Output "Android version metadata synchronized: versionName=$version; versionCode=$versionCode"
