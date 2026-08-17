param(
    [string]$StagingRoot
)
$ErrorActionPreference = 'Stop'
$ProjectRoot = Split-Path -Parent $PSScriptRoot
$MetadataPath = Join-Path $PSScriptRoot 'xray-core.json'
$metadata = Get-Content -LiteralPath $MetadataPath -Raw -Encoding UTF8 | ConvertFrom-Json
$sidecarPath = Join-Path $ProjectRoot 'src-tauri\binaries\xray-x86_64-pc-windows-msvc.exe'
if ([string]::IsNullOrWhiteSpace($StagingRoot)) { $StagingRoot = Join-Path $ProjectRoot '.tmp-xray' }
$work = Join-Path $StagingRoot ([Guid]::NewGuid().ToString('N'))
$archive = Join-Path $work 'xray.zip'
$extracted = Join-Path $work 'xray.exe'
try {
    New-Item -ItemType Directory -Path $work -Force | Out-Null
    Invoke-WebRequest -Uri $metadata.archiveUrl -OutFile $archive -UseBasicParsing
    $archiveHash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($archiveHash -cne $metadata.archiveSha256) { throw "Xray archive SHA-256 mismatch: $archiveHash" }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::OpenRead($archive)
    try {
        $entry = $zip.Entries | Where-Object { $_.FullName -ceq $metadata.archiveMember }
        if ($null -eq $entry -or @($entry).Count -ne 1) { throw "Xray archive member '$($metadata.archiveMember)' was not found as an exact root entry" }
        $input = $entry.Open(); $output = [System.IO.File]::Create($extracted)
        try { $input.CopyTo($output) } finally { $output.Dispose(); $input.Dispose() }
    } finally { $zip.Dispose() }
    $file = Get-Item -LiteralPath $extracted
    if ($file.Length -ne [int64]$metadata.executableSize) { throw "Xray executable size mismatch: $($file.Length)" }
    $executableHash = (Get-FileHash -LiteralPath $extracted -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($executableHash -cne $metadata.executableSha256) { throw "Xray executable SHA-256 mismatch: $executableHash" }
    $version = (& $extracted version 2>&1 | Out-String).Trim()
    $expectedVersion = $metadata.version.TrimStart('v')
    if ($version -notmatch [regex]::Escape($expectedVersion)) { throw "Xray version output did not contain $($metadata.version): $version" }
    New-Item -ItemType Directory -Path (Split-Path -Parent $sidecarPath) -Force | Out-Null
    Copy-Item -LiteralPath $extracted -Destination $sidecarPath -Force
    Write-Host "Prepared verified Xray sidecar: $sidecarPath"
} finally {
    if (Test-Path -LiteralPath $work) { Remove-Item -LiteralPath $work -Recurse -Force }
    if ((Test-Path -LiteralPath $StagingRoot -PathType Container) -and (@(Get-ChildItem -LiteralPath $StagingRoot -Force).Count -eq 0)) { Remove-Item -LiteralPath $StagingRoot -Force }
}
