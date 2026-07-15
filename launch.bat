@echo off
REM ============================================================================
REM Ghostlink Studio - Cinematic Launch Script (Windows)
REM Native launch - no Docker required.
REM Auto-detects hardware and configures the optimal inference backend.
REM ============================================================================

setlocal enabledelayedexpansion

REM Enable ANSI color processing
reg add HKCU\Console /v VirtualTerminalLevel /t REG_DWORD /d 1 /f >nul 2>&1

for /f %%a in ('echo prompt $e ^| cmd') do set "ESC=%%a"
set "CYAN=%ESC%[96m"
set "GREEN=%ESC%[92m"
set "YELLOW=%ESC%[93m"
set "RED=%ESC%[91m"
set "BLUE=%ESC%[94m"
set "MAGENTA=%ESC%[95m"
set "WHITE=%ESC%[97m"
set "GRAY=%ESC%[90m"
set "BOLD=%ESC%[1m"
set "DIM=%ESC%[2m"
set "NC=%ESC%[0m"

cls

REM ──────────────── BANNER ────────────────
echo.
echo %CYAN%  ╔══════════════════════════════════════════════════════════════════════════════╗%NC%
echo %CYAN%  ║%NC%  %WHITE%%BOLD%███████╗██╗  ██╗ ██████╗ ███████╗████████╗██╗     ██╗███╗   ██╗██╗  ██╗%CYAN%    ║%NC%
echo %CYAN%  ║%NC%  %WHITE%%BOLD%██╔════╝██║  ██║██╔═══██╗██╔════╝╚══██╔══╝██║     ██║████╗  ██║██║ ██╔╝%CYAN%    ║%NC%
echo %CYAN%  ║%NC%  %WHITE%%BOLD%█████╗  ███████║██║   ██║███████╗   ██║   ██║     ██║██╔██╗ ██║█████╔╝ %CYAN%    ║%NC%
echo %CYAN%  ║%NC%  %WHITE%%BOLD%██╔══╝  ██╔══██║██║   ██║╚════██║   ██║   ██║     ██║██║╚██╗██║██╔═██╗ %CYAN%    ║%NC%
echo %CYAN%  ║%NC%  %WHITE%%BOLD%██║     ██║  ██║╚██████╔╝███████║   ██║   ███████╗██║██║ ╚████║██║  ██╗%CYAN%    ║%NC%
echo %CYAN%  ║%NC%  %WHITE%%BOLD%╚═╝     ╚═╝  ╚═╝ ╚═════╝ ╚══════╝   ╚═╝   ╚══════╝╚═╝╚═╝  ╚═══╝╚═╝  ╚═╝%CYAN%    ║%NC%
echo %CYAN%  ║%NC%  %GRAY%Distributed LLM Inference Fabric - Native Mode%NC%                      %CYAN%║%NC%
echo %CYAN%  ╚═════════════════════════════════════════════════════════════════════════════════════╝%NC%
echo.

REM ──────────────── HARDWARE DETECTION ────────────────
echo %BLUE%%BOLD%  ⚡ Hardware Detection%NC%
echo.

set "GPU_DETECTED="
set "GPU_NAME="
set "NPU_DETECTED="
set "GPU_VENDOR="
set "BACKEND="

REM Check GPU via PowerShell
for /f "delims=" %%a in ('powershell -NoProfile -Command "Get-CimInstance Win32_VideoController | Where-Object { $_.Name -notmatch 'Microsoft Basic Display' } | ForEach-Object { $_.Name }" 2^>nul') do (
    set "GPU_NAME=%%a"
    set "GPU_DETECTED=1"
)

REM Check NVIDIA via nvidia-smi
where nvidia-smi >nul 2>&1
if not errorlevel 1 (
    for /f "tokens=*" %%a in ('nvidia-smi --query-gpu=name --format=csv,noheader 2^>nul') do set "NVIDIA_GPU=%%a"
    if defined NVIDIA_GPU (
        set "GPU_VENDOR=nvidia"
        set "BACKEND=CUDA"
    )
)

REM Check AMD via rocm-smi or WMI
if not defined GPU_VENDOR (
    where rocm-smi >nul 2>&1
    if not errorlevel 1 set "GPU_VENDOR=amd" && set "BACKEND=ROCm"
)
if not defined GPU_VENDOR (
    echo !GPU_NAME! | findstr /i "amd radeon" >nul
    if not errorlevel 1 set "GPU_VENDOR=amd" && set "BACKEND=DirectML"
)

REM Check Intel
if not defined GPU_VENDOR (
    echo !GPU_NAME! | findstr /i "intel iris intel arc" >nul
    if not errorlevel 1 set "GPU_VENDOR=intel" && set "BACKEND=DirectML"
)

REM Check NPU via WMI
type nul > "%TEMP%\ghostlink_npu.txt" 2>nul
powershell -NoProfile -Command "& {Get-CimInstance -ClassName Win32_PnPEntity -ErrorAction SilentlyContinue | Where-Object { $_.Name -match '\b(NPU|Neural Processor|AI Accelerator|XDNA|Ryzen AI)\b' } | ForEach-Object { Write-Output $_.Name }}" > "%TEMP%\ghostlink_npu.txt" 2>nul
set "NPU_DETECTED="
for /f "delims=" %%a in ('type "%TEMP%\ghostlink_npu.txt" 2^>nul') do (
    if not "%%a"=="" (
        echo %%a | findstr /i "keyboard mouse hid usb input" >nul
        if errorlevel 1 set "NPU_DETECTED=%%a"
    )
)
del "%TEMP%\ghostlink_npu.txt" 2>nul

REM Check CPU / RAM
for /f %%a in ('powershell -NoProfile -Command "(Get-CimInstance Win32_ComputerSystem).NumberOfLogicalProcessors" 2^>nul') do set "CPU_CORES=%%a"
for /f %%a in ('powershell -NoProfile -Command "[Math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1GB)" 2^>nul') do set "RAM_GB=%%a"

if defined GPU_DETECTED (
    echo    %GREEN%GPU%NC%       %GPU_NAME%
    if defined BACKEND echo    %GREEN%Backend%NC%    %BACKEND%
) else (
    echo    %YELLOW%GPU%NC%       Not detected ^(CPU mode^)
)
if defined NPU_DETECTED (
    echo    %MAGENTA%NPU%NC%       !NPU_DETECTED!
)
echo    %BLUE%CPU%NC%       !CPU_CORES! cores%NC%
if defined RAM_GB (
    echo    %BLUE%RAM%NC%       !RAM_GB! GB
)
echo.

REM ──────────────── LAUNCH NATIVE ────────────────
echo %CYAN%%BOLD%  🚀 Starting Ghostlink Studio (Native Mode)%NC%
echo.
echo %DIM%  Launching native inference stack...%NC%
echo.

call "%~dp0launch-complete.bat"
exit /b %errorlevel%
