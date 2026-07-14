@echo off
setlocal enabledelayedexpansion

REM Ghostlink Studio - Splash Screen with Hardware Detection (Windows)
REM Now delegates to launch.bat for native stack launch.

title Ghostlink Studio - Launching...

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
echo %CYAN%  ╔═══════════════════════════════════════════════════════════════════════════════════╗%NC%
echo %CYAN%  ║%NC%  %WHITE%%BOLD%███████╗██╗  ██╗ ██████╗ ███████╗████████╗██╗     ██╗███╗   ██╗██╗  ██╗%CYAN%    ║%NC%
echo %CYAN%  ║%NC%  %WHITE%%BOLD%██╔════╝██║  ██║██╔═══██╗██╔════╝╚══██╔══╝██║     ██║████╗  ██║██║ ██╔╝%CYAN%    ║%NC%
echo %CYAN%  ║%NC%  %WHITE%%BOLD%█████╗  ███████║██║   ██║███████╗   ██║   ██║     ██║██╔██╗ ██║█████╔╝ %CYAN%    ║%NC%
echo %CYAN%  ║%NC%  %WHITE%%BOLD%██╔══╝  ██╔══██║██║   ██║╚════██║   ██║   ██║     ██║██║╚██╗██║██╔═██╗ %CYAN%    ║%NC%
echo %CYAN%  ║%NC%  %WHITE%%BOLD%██║     ██║  ██║╚██████╔╝███████║   ██║   ███████╗██║██║ ╚████║██║  ██╗%CYAN%    ║%NC%
echo %CYAN%  ║%NC%  %WHITE%%BOLD%╚═╝     ╚═╝  ╚═╝ ╚═════╝ ╚══════╝   ╚═╝   ╚══════╝╚═╝╚═╝  ╚═══╝╚═╝  ╚═╝%CYAN%    ║%NC%
echo %CYAN%  ║%NC%  %GRAY%Distributed LLM Inference Fabric%NC%                                   %CYAN%║%NC%
echo %CYAN%  ╚═════════════════════════════════════════════════════════════════════════════════════╝%NC%
echo.

REM ──────────────── HARDWARE DETECTION ────────────────
echo %BLUE%%BOLD%  ⚡ Hardware Detection%NC%
echo.

set "GPU_DETECTED="
set "GPU_NAME="
set "NPU_DETECTED="

REM Check GPU via PowerShell
for /f "delims=" %%a in ('powershell -NoProfile -Command "Get-CimInstance Win32_VideoController | Where-Object { $_.Name -notmatch 'Microsoft Basic Display' } | ForEach-Object { $_.Name }" 2^>nul') do (
    set "GPU_NAME=%%a"
    set "GPU_DETECTED=1"
)

REM Check NPU via WMI (AMD XDNA / Intel NPU)
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

REM ──────────────── COMPONENT CHECKS ────────────────
echo %BLUE%%BOLD%  ✓ Component Check%NC%
echo.

if exist "target\release\ghost-link.exe" (
    echo    %GREEN%[OK]%NC%  Backend binary    %DIM%^(target\release\ghost-link.exe^)%NC%
) else if exist "target\debug\ghost-link.exe" (
    echo    %YELLOW%[--]%NC%  Backend binary    %DIM%^(debug build^)%NC%
) else (
    echo    %YELLOW%[--]%NC%  Backend binary    %DIM%^(will compile^)%NC%
)

if exist "third_party\llama.cpp\build\bin\Release\llama-server.exe" (
    echo    %GREEN%[OK]%NC%  llama-server      %DIM%^(built^)%NC%
) else if exist "third_party\llama.cpp\build\bin\llama-server.exe" (
    echo    %GREEN%[OK]%NC%  llama-server      %DIM%^(built^)%NC%
) else (
    echo    %YELLOW%[--]%NC%  llama-server      %DIM%^(will build^)%NC%
)

if exist "ghostlink_gui_modern\package.json" (
    echo    %GREEN%[OK]%NC%  React GUI         %DIM%^(ghostlink_gui_modern/^)%NC%
) else (
    echo    %RED%[XX]%NC%  React GUI         %DIM%^(ghostlink_gui_modern/ not found^)%NC%
)

echo.
echo %GREEN%%BOLD%  ✓ Ready to Launch%NC%
echo.
echo    %WHITE%Backend%NC%       http://127.0.0.1:8003
echo    %WHITE%Frontend%NC%      http://127.0.0.1:5173
echo    %WHITE%Inference%NC%     http://127.0.0.1:8080
echo.

echo %CYAN%  Starting native stack...%NC%
echo.

call "%~dp0launch-complete.bat"
exit /b %errorlevel%
