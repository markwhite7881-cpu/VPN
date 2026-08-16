param(
    [Parameter(Mandatory = $true)]
    [string]$Version,

    [Parameter(Mandatory = $true)]
    [string]$DistPath,

    [Parameter(Mandatory = $true)]
    [string]$ArtifactName,

    [Parameter(Mandatory = $true)]
    [string]$Platform,

    [Parameter(Mandatory = $true)]
    [string]$BaseUrl,

    [Parameter(Mandatory = $true)]
    [string]$SignaturePath,

    [string]$OutputPath = (Join-Path $DistPath 'latest.json')
)

$ErrorActionPreference = 'Stop'
$expectedArtifactName = "Cloakwire_$Version`_x64-setup.exe"
if ($ArtifactName -cne $expectedArtifactName) {
    throw "Artifact '$ArtifactName' does not have the required version token/name '$expectedArtifactName'."
}

$artifactPath = Join-Path $DistPath $ArtifactName
if (-not (Test-Path -LiteralPath $artifactPath -PathType Leaf)) {
    throw "Artifact is missing: $artifactPath"
}
if (-not (Test-Path -LiteralPath $SignaturePath -PathType Leaf)) {
    throw "Signature is missing: $SignaturePath"
}

$baseUri = $null
if (-not [Uri]::TryCreate($BaseUrl.TrimEnd('/'), [UriKind]::Absolute, [ref]$baseUri) -or $baseUri.Scheme -ne 'https') {
    throw "BaseUrl must be an absolute HTTPS URL: $BaseUrl"
}

$sigText = Get-Content -Raw -Encoding UTF8 -LiteralPath $SignaturePath
if ([string]::IsNullOrWhiteSpace($sigText)) {
    throw "Signature is empty: $SignaturePath"
}

$utf8 = [Text.UTF8Encoding]::new($false)
$manifest = [ordered]@{
    version = $Version
    notes = "Release v$Version - see the GitHub release notes for the full changelog."
    pub_date = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = [ordered]@{
        $Platform = [ordered]@{
            url = "$($baseUri.AbsoluteUri.TrimEnd('/'))/$ArtifactName"
            signature = [Convert]::ToBase64String($utf8.GetBytes($sigText))
        }
    }
} | ConvertTo-Json -Depth 8

[IO.File]::WriteAllText($OutputPath, $manifest, $utf8)
Write-Output "Wrote updater manifest: $OutputPath"
