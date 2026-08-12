@echo off
REM =============================================================
REM  Singbox Client — Release runner
REM
REM  Uses the standalone .exe from target\release (no tauri dev,
REM  no file watcher, no restarts). If the release build is missing
REM  it runs `npm run tauri:build` first, which can take 5-10 min.
REM =============================================================

setlocal
set "ROOT=%~dp0"
pushd "%ROOT%"

set "EXE=%ROOT%src-tauri\target\release\singbox-client.exe"

if not exist "%EXE%" (
    echo [run-release] release binary not found, building it now...
    call npm run tauri:build
    if errorlevel 1 (
        echo [run-release] build failed
        popd
        exit /b 1
    )
)

if not exist "%EXE%" (
    echo [run-release] still missing: %EXE%
    popd
    exit /b 1
)

echo [run-release] launching %EXE%
"%EXE%"

popd
endlocal
