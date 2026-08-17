param(
    [Parameter(Mandatory = $true)]
    [string]$ApkPath,

    [Parameter(Mandatory = $true)]
    [string]$ReferenceApkPath,

    [Parameter(Mandatory = $true)]
    [string]$ExpectedVersionName,

    [Parameter(Mandatory = $true)]
    [int]$ExpectedVersionCode,

    [Parameter(Mandatory = $true)]
    [string]$ExpectedPackage,

    [Parameter(Mandatory = $true)]
    [string]$ExpectedAbi,

    [Parameter(Mandatory = $true)]
    [string]$ExpectedCertificateSha256
)

$ErrorActionPreference = 'Stop'

function Get-LatestAndroidBuildTools {
    $roots = @()
    foreach ($sdkRoot in @($env:ANDROID_HOME, $env:ANDROID_SDK_ROOT)) {
        if (-not [string]::IsNullOrWhiteSpace($sdkRoot)) {
            $roots += Join-Path $sdkRoot 'build-tools'
        }
    }
    $roots += Join-Path $env:LOCALAPPDATA 'Android\Sdk\build-tools'
    $roots += Join-Path $env:USERPROFILE 'AppData\Local\Android\Sdk\build-tools'

    $candidates = foreach ($root in ($roots | Select-Object -Unique)) {
        if (-not (Test-Path -LiteralPath $root -PathType Container)) {
            continue
        }
        foreach ($directory in (Get-ChildItem -LiteralPath $root -Directory)) {
            $aapt = Join-Path $directory.FullName 'aapt.exe'
            $apksigner = Join-Path $directory.FullName 'lib\apksigner.jar'
            if ((Test-Path -LiteralPath $aapt -PathType Leaf) -and (Test-Path -LiteralPath $apksigner -PathType Leaf)) {
                [PSCustomObject]@{
                    Path = $directory.FullName
                    Version = $directory.Name
                    AaptPath = $aapt
                    ApkSignerPath = $apksigner
                }
            }
        }
    }

    $selected = $candidates | Sort-Object @{ Expression = {
        $parts = $_.Version -split '[^0-9]+' | Where-Object { $_ -ne '' }
        ('{0:D6}{1:D6}{2:D6}' -f [int]$parts[0], [int]$parts[1], [int]$parts[2])
    }; Descending = $true } | Select-Object -First 1
    if ($null -eq $selected) {
        throw 'Android build-tools with aapt.exe and lib\apksigner.jar were not found. Set ANDROID_HOME or ANDROID_SDK_ROOT to an Android SDK installation.'
    }
    return $selected
}

function Get-Java17Path {
    $java = Get-Command java.exe -ErrorAction SilentlyContinue
    if ($null -eq $java) {
        $java = Get-Command java -ErrorAction SilentlyContinue
    }
    if ($null -eq $java) {
        throw 'Java 17 was not found on PATH.'
    }

    $savedErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $versionOutput = & $java.Source -version 2>&1
        $javaExitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $savedErrorActionPreference
    }
    if ($javaExitCode -ne 0) {
        throw "Java version check failed with exit code $javaExitCode."
    }
    if (-not (($versionOutput | Out-String) -match '(?m)version "17\.')) {
        throw "Java 17 is required; found: $(($versionOutput | Select-Object -First 1).ToString())"
    }
    return $java.Source
}

function Invoke-Tool {
    param(
        [Parameter(Mandatory = $true)] [string]$FilePath,
        [Parameter(Mandatory = $true)] [string[]]$Arguments,
        [Parameter(Mandatory = $true)] [string]$Description
    )

    $savedErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $output = & $FilePath @Arguments 2>&1
        $toolExitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $savedErrorActionPreference
    }
    if ($toolExitCode -ne 0) {
        throw "$Description failed with exit code $toolExitCode."
    }
    return @($output | ForEach-Object { $_.ToString() })
}

function Get-ApkInventory {
    param(
        [Parameter(Mandatory = $true)] [string]$InspectionApkPath,
        [Parameter(Mandatory = $true)] [string]$OriginalApkPath,
        [Parameter(Mandatory = $true)] [string]$AaptPath,
        [Parameter(Mandatory = $true)] [string]$JavaPath,
        [Parameter(Mandatory = $true)] [string]$ApkSignerPath
    )

    $badging = Invoke-Tool -FilePath $AaptPath -Arguments @('dump', 'badging', $InspectionApkPath) -Description 'aapt dump badging'
    $packageLine = $badging | Where-Object { $_ -like 'package:*' } | Select-Object -First 1
    if ([string]::IsNullOrWhiteSpace($packageLine) -or $packageLine -notmatch "name='([^']+)'\s+versionCode='([^']+)'\s+versionName='([^']*)'") {
        throw 'aapt output did not contain a parseable package/version line.'
    }
    $packageName = $Matches[1]
    $versionCode = $Matches[2]
    $versionName = $Matches[3]

    $nativeLine = $badging | Where-Object { $_ -like 'native-code:*' } | Select-Object -First 1
    if ([string]::IsNullOrWhiteSpace($nativeLine)) {
        throw 'aapt output did not contain native-code; APK must declare a native ABI.'
    }
    $abis = @([regex]::Matches($nativeLine, "'([^']+)'") | ForEach-Object { $_.Groups[1].Value })
    if ($abis.Count -eq 0) {
        throw 'aapt native-code output did not contain an ABI.'
    }

    $signer = Invoke-Tool -FilePath $JavaPath -Arguments @('-jar', $ApkSignerPath, 'verify', '--verbose', '--print-certs', $InspectionApkPath) -Description 'apksigner verify'
    $signerText = $signer -join "`n"
    foreach ($scheme in @('v2', 'v3')) {
        if ($signerText -notmatch "Verified using $scheme scheme \(APK Signature Scheme $scheme\): true") {
            throw "APK is not verified using required $scheme signing scheme."
        }
    }
    $certificateMatches = [regex]::Matches($signerText, '(?im)^\s*(?:V\d+(?:\.\d+)?\s+)?Signer(?:\s+#\d+)?\s*:?\s*certificate SHA-256 digest:\s*(?<digest>\S*)\s*$')
    $certificateSha256s = @($certificateMatches | ForEach-Object {
        $digest = $_.Groups['digest'].Value
        if ($digest -notmatch '^[0-9a-fA-F]{64}$') {
            throw 'apksigner output contained a malformed signer SHA-256 certificate digest.'
        }
        $digest.ToLowerInvariant()
    } | Sort-Object -Unique)
    if ($certificateSha256s.Count -eq 0) {
        throw 'apksigner output did not contain a signer SHA-256 certificate digest.'
    }

    $entries = Invoke-Tool -FilePath $AaptPath -Arguments @('list', $InspectionApkPath) -Description 'aapt list'
    $stableEntries = @($entries | Where-Object {
        $_ -notmatch '^(META-INF/|stamp-cert-sha256$|BUNDLE-METADATA/)'
    } | Sort-Object -Unique)
    $inventoryText = $stableEntries -join "`n"
    $inventoryBytes = [Text.Encoding]::UTF8.GetBytes($inventoryText)
    $inventoryHash = ([Security.Cryptography.SHA256]::Create().ComputeHash($inventoryBytes) | ForEach-Object { $_.ToString('x2') }) -join ''

    [PSCustomObject]@{
        PackageName = $packageName
        VersionName = $versionName
        VersionCode = $versionCode
        Abis = $abis
        CertificateSha256s = $certificateSha256s
        StableEntryCount = $stableEntries.Count
        StableEntryHash = $inventoryHash
        FileHash = (Get-FileHash -LiteralPath $OriginalApkPath -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

foreach ($path in @($ApkPath, $ReferenceApkPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "APK input is missing: $path"
    }
}

$buildTools = Get-LatestAndroidBuildTools
$javaPath = Get-Java17Path
$projectRoot = Split-Path -Parent $PSScriptRoot
$scratchRoot = Join-Path $projectRoot 'release-staging\local-scratch'
$scratchPath = Join-Path $scratchRoot ('apk-verifier-' + [Guid]::NewGuid().ToString('N'))
$substDrive = $null

try {
    New-Item -ItemType Directory -Path $scratchPath -Force | Out-Null
    $occupiedDrives = @(Get-PSDrive -PSProvider FileSystem | ForEach-Object { $_.Name.ToUpperInvariant() })
    $driveLetter = @('Z', 'Y', 'X', 'W', 'V') | Where-Object { $occupiedDrives -notcontains $_ } | Select-Object -First 1
    if ([string]::IsNullOrWhiteSpace($driveLetter)) {
        throw 'No temporary drive letter is available for Android build-tools.'
    }
    $substDrive = "$driveLetter`:"
    & subst.exe $substDrive $scratchPath
    if ($LASTEXITCODE -ne 0) {
        throw "Unable to map temporary inspection drive $substDrive."
    }

    $candidateInspectionPath = Join-Path $substDrive 'candidate.apk'
    $referenceInspectionPath = Join-Path $substDrive 'reference.apk'
    Copy-Item -LiteralPath $ApkPath -Destination $candidateInspectionPath -ErrorAction Stop
    Copy-Item -LiteralPath $ReferenceApkPath -Destination $referenceInspectionPath -ErrorAction Stop

    $candidate = Get-ApkInventory -InspectionApkPath $candidateInspectionPath -OriginalApkPath $ApkPath -AaptPath $buildTools.AaptPath -JavaPath $javaPath -ApkSignerPath $buildTools.ApkSignerPath
    $reference = Get-ApkInventory -InspectionApkPath $referenceInspectionPath -OriginalApkPath $ReferenceApkPath -AaptPath $buildTools.AaptPath -JavaPath $javaPath -ApkSignerPath $buildTools.ApkSignerPath

    $failures = New-Object System.Collections.Generic.List[string]
    if ($candidate.PackageName -cne $ExpectedPackage) { $failures.Add("package expected $ExpectedPackage, got $($candidate.PackageName)") }
    if ($candidate.VersionName -cne $ExpectedVersionName) { $failures.Add("versionName expected $ExpectedVersionName, got $($candidate.VersionName)") }
    if ($candidate.VersionCode -cne $ExpectedVersionCode.ToString()) { $failures.Add("versionCode expected $ExpectedVersionCode, got $($candidate.VersionCode)") }
    if ($candidate.Abis -notcontains $ExpectedAbi) { $failures.Add("ABI expected $ExpectedAbi, got $($candidate.Abis -join ', ')") }
    if ($candidate.PackageName -cne $reference.PackageName) { $failures.Add('candidate package does not match reference package') }
    if (($candidate.Abis -join ',') -cne ($reference.Abis -join ',')) { $failures.Add('candidate native ABI inventory does not match reference') }
    $expectedCertificateSha256 = $ExpectedCertificateSha256.ToLowerInvariant()
    if (($candidate.CertificateSha256s -join ',') -cne ($reference.CertificateSha256s -join ',')) { $failures.Add('candidate signer certificate set does not match reference') }
    if ($candidate.CertificateSha256s -notcontains $expectedCertificateSha256) { $failures.Add('candidate signer certificate set does not contain expected certificate') }
    if ($candidate.StableEntryHash -cne $reference.StableEntryHash) { $failures.Add('candidate stable resource entry inventory does not match reference') }

    Write-Output "Android build-tools: $($buildTools.Version)"
    Write-Output "Candidate: package=$($candidate.PackageName); versionName=$($candidate.VersionName); versionCode=$($candidate.VersionCode); abi=$($candidate.Abis -join ','); signerCertificateSha256s=$($candidate.CertificateSha256s -join ','); fileSha256=$($candidate.FileHash); stableResourceEntries=$($candidate.StableEntryCount); stableResourceInventorySha256=$($candidate.StableEntryHash); signingSchemes=v2,v3"
    Write-Output "Reference: package=$($reference.PackageName); versionName=$($reference.VersionName); versionCode=$($reference.VersionCode); abi=$($reference.Abis -join ','); signerCertificateSha256s=$($reference.CertificateSha256s -join ','); fileSha256=$($reference.FileHash); stableResourceEntries=$($reference.StableEntryCount); stableResourceInventorySha256=$($reference.StableEntryHash); signingSchemes=v2,v3"

    if ($failures.Count -gt 0) {
        throw ('APK verification failed: ' + ($failures -join '; '))
    }
    Write-Output 'APK verification passed.'
} finally {
    if (-not [string]::IsNullOrWhiteSpace($substDrive)) {
        & subst.exe $substDrive /D 2>$null
    }
    if (Test-Path -LiteralPath $scratchPath) {
        Remove-Item -LiteralPath $scratchPath -Recurse -Force
    }
}
