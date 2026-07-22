@echo off
setlocal EnableExtensions EnableDelayedExpansion

REM Ghostlink unified launcher for Windows hosts.
REM This wrapper runs the Linux launcher inside WSL to keep one source of truth.

set "ROOT_DIR=%~dp0"
if "%ROOT_DIR:~-1%"=="\" set "ROOT_DIR=%ROOT_DIR:~0,-1%"
cd /d "%ROOT_DIR%"

where wsl >nul 2>nul
if errorlevel 1 (
    echo ERROR: WSL is required for this launcher but was not found.
    echo Install WSL and Ubuntu, then retry.
    echo Alternative: run launch.sh from Linux/WSL directly.
    exit /b 1
)

set "WSL_DISTRO_ARG="
if not "%GHOSTLINK_WSL_DISTRO%"=="" set "WSL_DISTRO_ARG=-d %GHOSTLINK_WSL_DISTRO%"

set "BACKEND_PREFIX="
if not "%GHOSTLINK_INFERENCE_BACKEND%"=="" set "BACKEND_PREFIX=GHOSTLINK_INFERENCE_BACKEND=%GHOSTLINK_INFERENCE_BACKEND% "

for /f "usebackq delims=" %%P in (`wsl %WSL_DISTRO_ARG% wslpath -a "%ROOT_DIR%"`) do set "WSL_ROOT_DIR=%%P"

if "%WSL_ROOT_DIR%"=="" (
    echo ERROR: Failed to resolve repository path inside WSL.
    exit /b 1
)

echo.
echo Launching Ghostlink via WSL from: %WSL_ROOT_DIR%
echo.

wsl %WSL_DISTRO_ARG% bash -lc "cd \"%WSL_ROOT_DIR%\" && %BACKEND_PREFIX%bash ./launch.sh"
set "RC=%ERRORLEVEL%"

if not "%RC%"=="0" (
    echo.
    echo Ghostlink launcher exited with code %RC%.
)

exit /b %RC%
