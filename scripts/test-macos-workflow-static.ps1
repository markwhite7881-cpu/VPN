$ErrorActionPreference = 'Stop'

$projectRoot = Split-Path -Parent $PSScriptRoot
$workflowPath = Join-Path $projectRoot '.github\workflows\release-macos.yml'
$workflow = [IO.File]::ReadAllText($workflowPath, [Text.UTF8Encoding]::new($false))

if ($workflow -match 'find .* -print -quit') {
    throw 'macOS workflow selects the first matching build artifact instead of rejecting ambiguous output.'
}

$required = @(
    'while IFS= read -r path; do dmgs+=("$path"); done < <(find "$dmg_dir" -maxdepth 1 -type f -name ''*.dmg'')',
    'while IFS= read -r path; do apps+=("$path"); done < <(find "$app_dir" -maxdepth 1 -type d -name ''*.app'')',
    '[ "${#dmgs[@]}" -eq 1 ] ||',
    '[ "${#apps[@]}" -eq 1 ] ||',
    'git fetch --depth 1 origin 9558ceb27bc1e92e2ecfa96ebfe6b3f688344c5a'
)
foreach ($needle in $required) {
    if (-not $workflow.Contains($needle)) {
        throw "macOS workflow is missing required exact-output/source-pinning guard: $needle"
    }
}

Write-Output 'PASS: macOS workflow rejects ambiguous artifacts and fetches the pinned source revision only.'
