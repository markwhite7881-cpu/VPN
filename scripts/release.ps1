# Build and stage one signed Windows updater artifact.
#
# This script intentionally does not publish anything. It uses a unique Cargo
# target directory, so the staged installer can only originate from this run.
#
# Usage:
#   .\scripts\release.ps1 -Version 1.2.1
#
param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^\d+\.\d+\.\d+([-.][0-9A-Za-z.-]+)?$')]
    [string]$Version,

    [string]$DistRoot,

    [string]$BaseUrl = 'https://github.com/markwhite7881-cpu/cloakwire/releases/download'
)

$ErrorActionPreference = 'Stop'
$ProjectRoot = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($DistRoot)) {
    $DistRoot = Join-Path $ProjectRoot 'release-staging'
}

$KeyPath = Join-Path $ProjectRoot 'src-tauri\.tauri-updater.key'
$SignerExe = Join-Path $ProjectRoot 'src-tauri\crates\tauri-signer\target\release\tauri-signer.exe'
$StagePath = Join-Path $DistRoot "v$Version"
$ManifestPath = Join-Path $StagePath 'latest.json'
$ManifestWriter = Join-Path $PSScriptRoot 'write-latest-json.ps1'
$Validator = Join-Path $PSScriptRoot 'validate-release.ps1'
$BuildTarget = Join-Path $DistRoot ('.build-v{0}-{1}' -f $Version, [Guid]::NewGuid().ToString('N'))
$NsisDir = Join-Path $BuildTarget 'release\bundle\nsis'
$ExpectedArtifactName = "Cloakwire_$Version`_x64-setup.exe"

foreach ($required in @($SignerExe, $KeyPath, $ManifestWriter, $Validator)) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
        throw "Required release input is missing: $required"
    }
}
if (Test-Path -LiteralPath $StagePath) {
    throw "Refusing to reuse existing staging directory: $StagePath"
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
    if (Test-Path -LiteralPath $cargoBin -PathType Container) {
        $env:PATH = "$cargoBin;$env:PATH"
    }
}

New-Item -ItemType Directory -Path $BuildTarget | Out-Null
try {
    Push-Location $ProjectRoot
    try {
        $env:CARGO_TARGET_DIR = $BuildTarget
        npm run tauri:build 2>&1 | Select-Object -Last 20
        $buildExit = $LASTEXITCODE
        if ($buildExit -ne 0) {
            throw "tauri build failed with exit code $buildExit"
        }
    } finally {
        Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
        Pop-Location
    }

    $sourcePath = Join-Path $NsisDir $ExpectedArtifactName
    if (-not (Test-Path -LiteralPath $sourcePath -PathType Leaf)) {
        throw "Fresh build did not produce expected NSIS installer: $sourcePath"
    }
    $unexpectedInstallers = @(Get-ChildItem -LiteralPath $NsisDir -File -Filter '*.exe')
    if ($unexpectedInstallers.Count -ne 1) {
        throw "Fresh build produced $($unexpectedInstallers.Count) NSIS installers; expected exactly one."
    }

    New-Item -ItemType Directory -Path $StagePath | Out-Null
    $artifactPath = Join-Path $StagePath $ExpectedArtifactName
    Copy-Item -LiteralPath $sourcePath -Destination $artifactPath -ErrorAction Stop

    & $SignerExe -k $KeyPath $artifactPath 2>&1 | Select-Object -Last 10
    $signExit = $LASTEXITCODE
    if ($signExit -ne 0) {
        throw "Updater signer failed with exit code $signExit"
    }

    $signaturePath = "$artifactPath.sig"
    if (-not (Test-Path -LiteralPath $signaturePath -PathType Leaf)) {
        throw "Updater signature was not created: $signaturePath"
    }

    $tagBaseUrl = "$($BaseUrl.TrimEnd('/'))/v$Version"
    & $ManifestWriter `
        -Version $Version `
        -DistPath $StagePath `
        -ArtifactName $ExpectedArtifactName `
        -Platform 'windows-x86_64' `
        -BaseUrl $tagBaseUrl `
        -SignaturePath $signaturePath `
        -OutputPath $ManifestPath

    & $Validator `
        -ManifestPath $ManifestPath `
        -DistPath $StagePath `
        -Version $Version `
        -RequiredPlatforms 'windows-x86_64'

    Write-Host ''
    Write-Host '==> Staged and validated. Review before publishing:' -ForegroundColor Green
    Get-ChildItem -LiteralPath $StagePath -File | ForEach-Object { Write-Host "    $($_.FullName)" -ForegroundColor Green }
    Write-Host ''
    Write-Host "  gh release create v$Version `"$artifactPath`" `"$signaturePath`" `"$ManifestPath`" --title `"v$Version`" --generate-notes" -ForegroundColor Yellow
    Write-Host ''
    Write-Host '==> This script does not publish the release.' -ForegroundColor Magenta
} finally {
    if (Test-Path -LiteralPath $BuildTarget) {
        Remove-Item -LiteralPath $BuildTarget -Recurse -Force
    }
}
