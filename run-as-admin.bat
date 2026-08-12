@echo off
REM Launches the singbox-client in Tauri dev mode with Administrator
REM privileges (right-click → "Run as administrator").
REM
REM Needed for TUN mode — sing-box must install the wintun driver
REM and create the TUN interface, both of which require admin.

setlocal

REM Make sure cargo is on PATH even when the admin shell has a
REM stripped-down environment.
set "PATH=C:\Users\Алексей\.cargo\bin;%PATH%"

REM Resolve to absolute path of the project (works regardless of CWD).
set "PROJECT_DIR=%~dp0"

echo ===============================================
echo   Singbox Client - Tauri dev (admin mode)
echo ===============================================
echo.
echo Project: %PROJECT_DIR%
echo.
echo Checking prerequisites...
where node >nul 2>&1
if errorlevel 1 (
  echo   [X] node.exe not found on PATH
  echo       Try running this from a normal PowerShell first
  echo       to set up the user PATH, then run as admin.
  goto :end
) else (
  echo   [OK] node.exe
)
where npm >nul 2>&1
if errorlevel 1 (
  echo   [X] npm.exe not found
  goto :end
) else (
  echo   [OK] npm.exe
)
where cargo >nul 2>&1
if errorlevel 1 (
  echo   [X] cargo.exe not found - Tauri cannot build
  echo       Path should include C:\Users\Алексей\.cargo\bin
  goto :end
) else (
  echo   [OK] cargo.exe
)
echo.
echo Starting tauri dev. Window stays open if something fails.
echo Press Ctrl+C in this window to stop.
echo.

cd /d "%PROJECT_DIR%"
call npm run tauri:dev

:end
echo.
echo Press any key to close this window...
pause >nul
endlocal
