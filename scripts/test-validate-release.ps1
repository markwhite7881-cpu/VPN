$ErrorActionPreference = 'Stop'

$projectRoot = Split-Path -Parent $PSScriptRoot
$validator = Join-Path $PSScriptRoot 'validate-release.ps1'
$manifestWriter = Join-Path $PSScriptRoot 'write-latest-json.ps1'
$fixtureRoot = Join-Path $projectRoot '.release-validation-fixtures'

function Remove-FixtureRoot {
    if (Test-Path -LiteralPath $fixtureRoot) {
        Remove-Item -LiteralPath $fixtureRoot -Recurse -Force
    }
}

function Write-Manifest([string]$path, [string]$version, [string]$fileName, [string]$signature = 'fixture-signature') {
    $manifest = @{
        version = $version
        platforms = @{
            'windows-x86_64' = @{
                url = "https://github.com/markwhite7881-cpu/cloakwire/releases/download/v$version/$fileName"
                signature = $signature
            }
        }
    } | ConvertTo-Json -Depth 5
    [IO.File]::WriteAllText($path, $manifest, [Text.UTF8Encoding]::new($false))
}

try {
    Remove-FixtureRoot
    New-Item -ItemType Directory -Path $fixtureRoot | Out-Null

    $stale = Join-Path $fixtureRoot 'stale'
    New-Item -ItemType Directory -Path $stale | Out-Null
    $staleFile = 'Cloakwire_1.2.0_x64-setup.exe'
    [IO.File]::WriteAllText((Join-Path $stale $staleFile), 'fixture', [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText((Join-Path $stale "$staleFile.sig"), 'fixture signature', [Text.UTF8Encoding]::new($false))
    $staleManifest = Join-Path $stale 'latest.json'
    Write-Manifest $staleManifest '1.2.1' $staleFile
    & $validator -ManifestPath $staleManifest -DistPath $stale -Version '1.2.1' -RequiredPlatforms 'windows-x86_64'
    throw 'Expected stale-artifact fixture to fail validation.'
} catch {
    if ($_.Exception.Message -notmatch 'does not have the required version token') { throw }
    Write-Output 'PASS: stale artifact is rejected.'
} finally {
    Remove-FixtureRoot
}

try {
    New-Item -ItemType Directory -Path $fixtureRoot | Out-Null
    $unsigned = Join-Path $fixtureRoot 'unsigned'
    New-Item -ItemType Directory -Path $unsigned | Out-Null
    $unsignedFile = 'Cloakwire_1.2.1_x64-setup.exe'
    [IO.File]::WriteAllText((Join-Path $unsigned $unsignedFile), 'fixture', [Text.UTF8Encoding]::new($false))
    $unsignedManifest = Join-Path $unsigned 'latest.json'
    Write-Manifest $unsignedManifest '1.2.1' $unsignedFile
    & $validator -ManifestPath $unsignedManifest -DistPath $unsigned -Version '1.2.1' -RequiredPlatforms 'windows-x86_64'
    throw 'Expected missing-signature fixture to fail validation.'
} catch {
    if ($_.Exception.Message -notmatch 'Signature sidecar is missing') { throw }
    Write-Output 'PASS: missing signature is rejected.'
} finally {
    Remove-FixtureRoot
}

try {
    New-Item -ItemType Directory -Path $fixtureRoot | Out-Null
    $prefix = Join-Path $fixtureRoot 'prefix-collision'
    New-Item -ItemType Directory -Path $prefix | Out-Null
    $prefixFile = 'Cloakwire_1.2.10_x64-setup.exe'
    [IO.File]::WriteAllText((Join-Path $prefix $prefixFile), 'fixture', [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText((Join-Path $prefix "$prefixFile.sig"), 'fixture signature', [Text.UTF8Encoding]::new($false))
    $prefixManifest = Join-Path $prefix 'latest.json'
    Write-Manifest $prefixManifest '1.2.1' $prefixFile ([Convert]::ToBase64String([Text.UTF8Encoding]::new($false).GetBytes('fixture signature')))
    & $validator -ManifestPath $prefixManifest -DistPath $prefix -Version '1.2.1' -RequiredPlatforms 'windows-x86_64'
    throw 'Expected version-prefix collision fixture to fail validation.'
} catch {
    if ($_.Exception.Message -notmatch 'does not have the required version token') { throw }
    Write-Output 'PASS: version-prefix collision is rejected.'
} finally {
    Remove-FixtureRoot
}

try {
    New-Item -ItemType Directory -Path $fixtureRoot | Out-Null
    $mismatch = Join-Path $fixtureRoot 'signature-mismatch'
    New-Item -ItemType Directory -Path $mismatch | Out-Null
    $mismatchFile = 'Cloakwire_1.2.1_x64-setup.exe'
    [IO.File]::WriteAllText((Join-Path $mismatch $mismatchFile), 'fixture', [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText((Join-Path $mismatch "$mismatchFile.sig"), 'fixture signature', [Text.UTF8Encoding]::new($false))
    $mismatchManifest = Join-Path $mismatch 'latest.json'
    Write-Manifest $mismatchManifest '1.2.1' $mismatchFile ([Convert]::ToBase64String([Text.UTF8Encoding]::new($false).GetBytes('different signature')))
    & $validator -ManifestPath $mismatchManifest -DistPath $mismatch -Version '1.2.1' -RequiredPlatforms 'windows-x86_64'
    throw 'Expected signature-mismatch fixture to fail validation.'
} catch {
    if ($_.Exception.Message -notmatch 'does not match staged sidecar') { throw }
    Write-Output 'PASS: manifest signature mismatch is rejected.'
} finally {
    Remove-FixtureRoot
}

$valid = Join-Path $fixtureRoot 'valid'
New-Item -ItemType Directory -Path $valid | Out-Null
$validFile = 'Cloakwire_1.2.1_x64-setup.exe'
[IO.File]::WriteAllText((Join-Path $valid $validFile), 'fixture', [Text.UTF8Encoding]::new($false))
[IO.File]::WriteAllText((Join-Path $valid "$validFile.sig"), 'fixture signature', [Text.UTF8Encoding]::new($false))
$validManifest = Join-Path $valid 'latest.json'
& $manifestWriter `
    -Version '1.2.1' `
    -DistPath $valid `
    -ArtifactName $validFile `
    -Platform 'windows-x86_64' `
    -BaseUrl 'https://github.com/markwhite7881-cpu/cloakwire/releases/download/v1.2.1' `
    -SignaturePath (Join-Path $valid "$validFile.sig") `
    -OutputPath $validManifest
& $validator -ManifestPath $validManifest -DistPath $valid -Version '1.2.1' -RequiredPlatforms 'windows-x86_64'
Write-Output 'PASS: complete release fixture is accepted.'
Remove-FixtureRoot
