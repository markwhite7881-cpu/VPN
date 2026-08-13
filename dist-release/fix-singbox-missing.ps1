# Cloakwire: восстановление sing-box.exe рядом с cloakwire.exe
#
# Симптом: при запуске Cloakwire получаем ошибку
#   binary_not_found: failed to locate sing-box binary:
#   expected one of sing-box-x86_64-pc-windows-msvc.exe, sing-box.exe
#
# Почти всегда это значит, что `sing-box.exe` (52 МБ) был удалён
# антивирусом — у VPN-тулзов это типичный false positive. Вторая
# частая причина — друг установил Cloakwire не через NSIS, а
# из portable-архива, где sing-box.exe нужно класть руками.
#
# Скрипт:
#   1. Находит, где установлен Cloakwire
#   2. Проверяет, есть ли sing-box рядом
#   3. Если нет — качает с GitHub Releases и кладёт рядом
#   4. Печатает инструкцию как добавить папку в исключения

$ErrorActionPreference = "Stop"

# --- helpers -------------------------------------------------------------

function Write-Section {
    param([string]$Title)
    Write-Host ""
    Write-Host "=== $Title ===" -ForegroundColor Cyan
}

function Find-CloakwireInstall {
    # Standard NSIS install path (v1.0.0+ identifier ru.classquiz.singbox).
    $candidates = @(
        Join-Path $env:LOCALAPPDATA "Cloakwire"
        # Older releases (v1.0.0 used "Singbox Client" as productName but
        # the same identifier, so the AppData dir was already Cloakwire.
        # Pre-v1.0.0 used a different identifier and a different dir.
        "C:\Program Files\Cloakwire"
        "C:\Program Files (x86)\Cloakwire"
    )
    foreach ($dir in $candidates) {
        $exe = Join-Path $dir "cloakwire.exe"
        if (Test-Path $exe) { return $dir }
    }
    return $null
}

function Get-SingboxUrl {
    # The same v1.0.3 NSIS installer also packages sing-box.exe.
    # We grab it from the latest release so the SHA matches the
    # locally-installed cloakwire.exe.
    # (Sing-box binaries are stable across versions; the SFA client
    # is what changes. We're not embedding SFA here — we just want
    # the sing-box sidecar from the GitHub Release asset.)
    # The asset name in the latest release is "Cloakwire_1.0.3_x64-setup.exe",
    # which is a NSIS installer — but we want just sing-box.exe.
    # Fortunately, sing-box is also published upstream on the SagerNet
    # sing-box repo as a separate binary. We pin to the same version
    # (1.14.0-lx.24) that's in our v1.0.3 local install (Cloakwire
    # bundles 1.14.0; v1.0.2 used 1.14.0-lx.24 too).
    return "https://github.com/SagerNet/sing-box/releases/download/v1.14.0/sing-box-1.14.0-windows-amd64.zip"
}

# --- main ----------------------------------------------------------------

Write-Section "Cloakwire: проверка sing-box.exe"

$installDir = Find-CloakwireInstall
if (-not $installDir) {
    Write-Host "Не нашёл установленный Cloakwire." -ForegroundColor Red
    Write-Host "Проверь вручную: ищи `cloakwire.exe` в Проводнике, или в %LOCALAPPDATA%\Cloakwire" -ForegroundColor Yellow
    exit 1
}

Write-Host "Cloakwire установлен в: $installDir" -ForegroundColor Green

$singboxPath = Join-Path $installDir "sing-box.exe"
$singboxLongPath = Join-Path $installDir "sing-box-x86_64-pc-windows-msvc.exe"
$longExists = Test-Path $singboxLongPath

if ((Test-Path $singboxPath) -or $longExists) {
    Write-Host "OK: sing-box уже на месте" -ForegroundColor Green
    if (Test-Path $singboxPath) {
        $size = (Get-Item $singboxPath).Length
        Write-Host "  $singboxPath ($([math]::Round($size/1MB,1)) MB)"
    }
    if ($longExists) {
        $size = (Get-Item $singboxLongPath).Length
        Write-Host "  $singboxLongPath ($([math]::Round($size/1MB,1)) MB)"
    }
    exit 0
}

Write-Host "sing-box.exe не найден — качаю с sing-box Releases..." -ForegroundColor Yellow

$url = Get-SingboxUrl
$zip = Join-Path $env:TEMP "sing-box.zip"
Write-Host "  $url"

try {
    Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing -ErrorAction Stop
} catch {
    Write-Host "Не удалось скачать: $_" -ForegroundColor Red
    Write-Host "Скачай руками: $url" -ForegroundColor Yellow
    Write-Host "  1. Скачай sing-box-1.14.0-windows-amd64.zip"
    Write-Host "  2. Распакуй (внутри sing-box-windows-amd64.exe)"
    Write-Host "  3. Переименуй в sing-box.exe"
    Write-Host "  4. Положи в $installDir"
    exit 1
}

# Extract. The upstream zip contains `sing-box-windows-amd64.exe` directly.
$tmp = Join-Path $env:TEMP "sing-box-extract"
if (Test-Path $tmp) { Remove-Item $tmp -Recurse -Force }
Expand-Archive -Path $zip -DestinationPath $tmp -Force

$extracted = Get-ChildItem -Path $tmp -Recurse -Filter "sing-box-windows-amd64.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $extracted) {
    $extracted = Get-ChildItem -Path $tmp -Recurse -Filter "sing-box.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
}
if (-not $extracted) {
    Write-Host "В архиве не нашёл sing-box-*.exe — посмотри в $tmp вручную" -ForegroundColor Red
    exit 1
}

Copy-Item $extracted.FullName -Destination $singboxPath -Force
Write-Host "Скопировал в: $singboxPath" -ForegroundColor Green
Write-Host "  size: $([math]::Round((Get-Item $singboxPath).Length / 1MB, 1)) MB"

Remove-Item $zip -Force
Remove-Item $tmp -Recurse -Force

Write-Section "Следующий шаг: исключение в антивирусе"

Write-Host "Если через 5 минут sing-box.exe снова пропадает — это антивирус."
Write-Host ""
Write-Host "Как добавить папку в исключения:"
Write-Host ""
Write-Host "  Windows Defender:"
Write-Host "    Settings > Privacy & Security > Windows Security > Virus & threat protection"
Write-Host "    > Virus & threat protection settings > Exclusions > Add or remove exclusions"
Write-Host "    > Add folder > $installDir"
Write-Host ""
Write-Host "  Kaspersky / ESET / Avast / DrWeb: см. документацию антивируса."
Write-Host ""
Write-Host "  Если sing-box.exe не удаляется, но Cloakwire всё равно не видит его —"
Write-Host "  закрой Cloakwire полностью (иконка в трее > Quit), удали sing-box.exe"
Write-Host "  из карантина антивируса, запусти Cloakwire заново — он пересоздаст процесс"
Write-Host "  и антивирус, возможно, перестанет его блокировать."

Write-Section "Готово"
Write-Host "Запусти Cloakwire заново. Если ошибка повторяется — это антивирус." -ForegroundColor Green
