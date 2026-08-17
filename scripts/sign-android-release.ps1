param(
    [Parameter(Mandatory = $true)]
    [string]$InputApkPath,

    [Parameter(Mandatory = $true)]
    [string]$OutputApkPath,

    [string]$ApksignerJarPath,

    [string]$JavaPath,

    [switch]$Force
)

$ErrorActionPreference = 'Stop'
$ProjectRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))

$requiredEnvironmentVariables = @(
    'CLOAKWIRE_ANDROID_KEYSTORE_PATH',
    'CLOAKWIRE_ANDROID_KEYSTORE_PASSWORD',
    'CLOAKWIRE_ANDROID_KEY_ALIAS',
    'CLOAKWIRE_ANDROID_KEY_PASSWORD'
)
$missingEnvironmentVariables = @(
    $requiredEnvironmentVariables | Where-Object {
        [string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($_, 'Process'))
    }
)
if ($missingEnvironmentVariables.Count -gt 0) {
    throw ('Missing required environment variables: ' + ($missingEnvironmentVariables -join ', '))
}

$inputPath = [System.IO.Path]::GetFullPath($InputApkPath)
$outputPath = [System.IO.Path]::GetFullPath($OutputApkPath)
if ($inputPath -eq $outputPath) {
    throw 'Output APK path must differ from input APK path.'
}
if ($outputPath.StartsWith($inputPath + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw 'Output APK path must not be inside the input APK path.'
}
if (-not (Test-Path -LiteralPath $inputPath -PathType Leaf)) {
    throw 'Input APK is missing.'
}
if ((Test-Path -LiteralPath $outputPath -PathType Leaf) -and -not $Force) {
    throw 'Output APK already exists. Refusing to overwrite without -Force.'
}

$keystorePath = [System.IO.Path]::GetFullPath([Environment]::GetEnvironmentVariable('CLOAKWIRE_ANDROID_KEYSTORE_PATH', 'Process'))
$projectRootPrefix = $ProjectRoot.TrimEnd([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
if ($keystorePath.StartsWith($projectRootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw 'Keystore path must be outside the repository.'
}
if (-not (Test-Path -LiteralPath $keystorePath -PathType Leaf)) {
    throw 'Keystore file is missing.'
}

if ([string]::IsNullOrWhiteSpace($ApksignerJarPath)) {
    $sdkRoot = if ($env:ANDROID_SDK_ROOT) { $env:ANDROID_SDK_ROOT } elseif ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { $null }
    if (-not $sdkRoot) {
        throw 'Android SDK root is unavailable; set ANDROID_SDK_ROOT or provide -ApksignerJarPath.'
    }
    $buildToolsRoot = Join-Path $sdkRoot 'build-tools'
    if (-not (Test-Path -LiteralPath $buildToolsRoot -PathType Container)) {
        throw 'Android SDK build-tools directory is missing.'
    }
    $apksignerCandidate = Get-ChildItem -LiteralPath $buildToolsRoot -Directory |
        Sort-Object Name -Descending |
        ForEach-Object { Join-Path $_.FullName 'lib\apksigner.jar' } |
        Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
        Select-Object -First 1
    if (-not $apksignerCandidate) {
        throw 'Could not locate apksigner.jar in Android SDK build-tools.'
    }
    $ApksignerJarPath = $apksignerCandidate
}
$apksignerJar = [System.IO.Path]::GetFullPath($ApksignerJarPath)
if (-not (Test-Path -LiteralPath $apksignerJar -PathType Leaf)) {
    throw 'apksigner.jar is missing.'
}

if ([string]::IsNullOrWhiteSpace($JavaPath)) {
    $javaCommand = Get-Command java -ErrorAction SilentlyContinue
    if (-not $javaCommand) {
        throw 'Java executable is unavailable; provide -JavaPath.'
    }
    $JavaPath = $javaCommand.Source
}
$javaExecutable = [System.IO.Path]::GetFullPath($JavaPath)
if (-not (Test-Path -LiteralPath $javaExecutable -PathType Leaf)) {
    throw 'Java executable is missing.'
}

$outputParent = Split-Path -Parent $outputPath
New-Item -ItemType Directory -Path $outputParent -Force | Out-Null

$processStartInfo = [System.Diagnostics.ProcessStartInfo]::new()
$processStartInfo.FileName = $javaExecutable
$processStartInfo.UseShellExecute = $false
$signingArguments = @(
    '-jar',
    $apksignerJar,
    'sign',
    '--ks',
    $keystorePath,
    '--ks-pass',
    'env:CLOAKWIRE_ANDROID_KEYSTORE_PASSWORD',
    '--ks-key-alias',
    [Environment]::GetEnvironmentVariable('CLOAKWIRE_ANDROID_KEY_ALIAS', 'Process'),
    '--key-pass',
    'env:CLOAKWIRE_ANDROID_KEY_PASSWORD',
    '--v1-signing-enabled',
    'false',
    '--v2-signing-enabled',
    'true',
    '--v3-signing-enabled',
    'true',
    '--out',
    $outputPath,
    $inputPath
)

if ($null -ne $processStartInfo.ArgumentList) {
    foreach ($argument in $signingArguments) {
        $processStartInfo.ArgumentList.Add($argument)
    }
} else {
    $processStartInfo.Arguments = ($signingArguments | ForEach-Object {
        '"' + ($_ -replace '(\\*)"', '$1$1\"') + '"'
    }) -join ' '
}

$signingProcess = [System.Diagnostics.Process]::Start($processStartInfo)
$signingProcess.WaitForExit()
if ($signingProcess.ExitCode -ne 0) {
    throw "APK signing failed with exit code $($signingProcess.ExitCode)."
}
if (-not (Test-Path -LiteralPath $outputPath -PathType Leaf)) {
    throw 'APK signing completed without producing the output file.'
}

Write-Host "Signed APK: $outputPath" -ForegroundColor Green
