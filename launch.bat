@echo off
setlocal EnableExtensions EnableDelayedExpansion

REM Ghostlink unified launcher for Windows hosts.
REM
REM Default: runs natively on Windows (launch-native.ps1) so real Windows GPU
REM detection and drivers (Vulkan/DirectML) are visible to the app. WSL2 runs
REM a Linux build in a lightweight VM that can't see this host's GPU the same
REM way, so it silently fell back to CPU-only and isn't the default anymore.
REM
REM Set GHOSTLINK_USE_WSL=1 to keep using the old WSL-delegated launch.sh path.

set "ROOT_DIR=%~dp0"
if "%ROOT_DIR:~-1%"=="\" set "ROOT_DIR=%ROOT_DIR:~0,-1%"
cd /d "%ROOT_DIR%"

if "%GHOSTLINK_USE_WSL%"=="1" goto :use_wsl

where powershell >nul 2>nul
if errorlevel 1 (
    echo ERROR: PowerShell is required for the native launcher but was not found.
    echo Set GHOSTLINK_USE_WSL=1 to fall back to the WSL-based launcher instead.
    exit /b 1
)

echo.
echo Launching Ghostlink natively on Windows...
echo.

powershell -NoProfile -ExecutionPolicy Bypass -File "%ROOT_DIR%\launch-native.ps1" -OpenBrowser %*
set "RC=%ERRORLEVEL%"

if not "%RC%"=="0" (
    echo.
    echo Ghostlink native launcher exited with code %RC%.
)

exit /b %RC%

:use_wsl
where wsl >nul 2>nul
if errorlevel 1 (
    echo ERROR: WSL is required for GHOSTLINK_USE_WSL=1 but was not found.
    echo Install WSL and Ubuntu, or unset GHOSTLINK_USE_WSL to use the native launcher.
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
