param(
    [Parameter(Mandatory = $true)]
    [string]$ManifestPath,

    [Parameter(Mandatory = $true)]
    [string]$DistPath,

    [Parameter(Mandatory = $true)]
    [string]$Version,

    [Parameter(Mandatory = $true)]
    [string[]]$RequiredPlatforms
)

$ErrorActionPreference = 'Stop'

$manifest = Get-Content -Raw -Encoding UTF8 -Path $ManifestPath | ConvertFrom-Json
if ($manifest.version -ne $Version) {
    throw "Manifest version '$($manifest.version)' does not match requested version '$Version'."
}

foreach ($platform in $RequiredPlatforms) {
    $entry = $manifest.platforms.$platform
    if ($null -eq $entry) {
        throw "Manifest is missing required platform '$platform'."
    }

    $uri = $null
    if (-not [Uri]::TryCreate([string]$entry.url, [UriKind]::Absolute, [ref]$uri) -or $uri.Scheme -ne 'https') {
        throw "Platform '$platform' has a non-HTTPS artifact URL '$($entry.url)'."
    }

    $fileName = [IO.Path]::GetFileName($uri.AbsolutePath)
    if ([string]::IsNullOrWhiteSpace($fileName)) {
        throw "Platform '$platform' URL does not name an artifact."
    }
    $expectedFileName = "Cloakwire_$Version`_x64-setup.exe"
    if ($fileName -cne $expectedFileName) {
        throw "Artifact '$fileName' for platform '$platform' does not have the required version token/name '$expectedFileName'."
    }

    $artifactPath = Join-Path $DistPath $fileName
    if (-not (Test-Path -LiteralPath $artifactPath -PathType Leaf)) {
        throw "Staged artifact is missing for platform '$platform': $artifactPath"
    }
    $signaturePath = "$artifactPath.sig"
    if (-not (Test-Path -LiteralPath $signaturePath -PathType Leaf)) {
        throw "Signature sidecar is missing for platform '$platform': $signaturePath"
    }
    if ([string]::IsNullOrWhiteSpace([string]$entry.signature)) {
        throw "Manifest signature is empty for platform '$platform'."
    }

    try {
        $manifestSignature = [Text.UTF8Encoding]::new($false).GetString([Convert]::FromBase64String([string]$entry.signature))
    } catch {
        throw "Manifest signature is not valid Base64 for platform '$platform'."
    }
    $sidecarSignature = Get-Content -Raw -Encoding UTF8 -LiteralPath $signaturePath
    if ($manifestSignature -cne $sidecarSignature) {
        throw "Manifest signature does not match staged sidecar for platform '$platform'."
    }
}

Write-Output "Release manifest '$ManifestPath' is valid for version '$Version'."
