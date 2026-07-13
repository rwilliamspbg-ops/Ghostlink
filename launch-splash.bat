@echo off
setlocal enabledelayedexpansion

REM ============================================================================
REM  Ghostlink Studio – Splash Screen with Hardware Detection (Windows)
REM ============================================================================

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
echo %CYAN%  ╔══════════════════════════════════════════════════════════════════════╗%NC%
echo %CYAN%  ║%NC%  %WHITE%%BOLD%███████╗██╗  ██╗ ██████╗ ███████╗████████╗██╗     ██╗███╗   ██╗██╗  ██╗%CYAN%    ║%NC%
echo %CYAN%  ║%NC%  %WHITE%%BOLD%██╔════╝██║  ██║██╔═══██╗██╔════╝╚══██╔══╝██║     ██║████╗  ██║██║ ██╔╝%CYAN%    ║%NC%
echo %CYAN%  ║%NC%  %WHITE%%BOLD%█████╗  ███████║██║   ██║███████╗   ██║   ██║     ██║██╔██╗ ██║█████╔╝ %CYAN%    ║%NC%
echo %CYAN%  ║%NC%  %WHITE%%BOLD%██╔══╝  ██╔══██║██║   ██║╚════██║   ██║   ██║     ██║██║╚██╗██║██╔═██╗ %CYAN%    ║%NC%
echo %CYAN%  ║%NC%  %WHITE%%BOLD%██║     ██║  ██║╚██████╔╝███████║   ██║   ███████╗██║██║ ╚████║██║  ██╗%CYAN%    ║%NC%
echo %CYAN%  ║%NC%  %WHITE%%BOLD%╚═╝     ╚═╝  ╚═╝ ╚═════╝ ╚══════╝   ╚═╝   ╚══════╝╚═╝╚═╝  ╚═══╝╚═╝  ╚═╝%CYAN%    ║%NC%
echo %CYAN%  ║%NC%  %GRAY%Distributed LLM Inference Fabric%NC%                                   %CYAN%║%NC%
echo %CYAN%  ╚══════════════════════════════════════════════════════════════════════╝%NC%
echo.

REM ──────────────── HARDWARE DETECTION ────────────────
echo %BLUE%%BOLD%  ⚡ Hardware Detection%NC%
echo.

set "GPU_DETECTED="
set "GPU_NAME="
set "NPU_DETECTED="

REM Check GPU via WMI
for /f "skip=1 tokens=2 delims=," %%a in ('wmic path Win32_VideoController get Name /format:csv 2^>nul') do (
    echo %%a | findstr /i "microsoft basic display" >nul
    if errorlevel 1 (
        set "GPU_NAME=%%a"
        set "GPU_DETECTED=1"
    )
)

REM Check NPU via WMI (AMD XDNA / Intel NPU)
powershell -NoProfile -Command "& {Get-CimInstance -ClassName Win32_PnPEntity -ErrorAction SilentlyContinue | Where-Object { $_.Name -match '(NPU|Neural|AI Accelerator|XDNA|Ryzen AI)' } | ForEach-Object { Write-Output $_.Name }}" > "%TEMP%\ghostlink_npu.txt" 2>nul
for /f "delims=" %%a in ('type "%TEMP%\ghostlink_npu.txt" 2^>nul') do (
    if not "%%a"=="" (
        set "NPU_DETECTED=%%a"
    )
)
del "%TEMP%\ghostlink_npu.txt" 2>nul

REM Check CPU / RAM
for /f "tokens=2 delims==" %%a in ('wmic cpu get NumberOfCores /value 2^>nul') do set "CPU_CORES=%%a"
for /f "tokens=2 delims==" %%a in ('wmic os get TotalVisibleMemorySize /value 2^>nul') do set /a "RAM_GB=%%a / 1048576" 2>nul

if defined GPU_DETECTED (
    echo    %GREEN%GPU%NC%       %GPU_NAME%
) else (
    echo    %YELLOW%GPU%NC%       Not detected (CPU mode)
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

set "BACKEND_OK=0"
set "LLAMA_OK=0"
set "MODEL_OK=0"
set "GUI_OK=0"

if exist "target\release\ghost-link.exe" (
   echo    %GREEN%[OK]%NC%  Backend binary    %DIM%(target\release\ghost-link.exe)%NC%
    set "BACKEND_OK=1"
) else if exist "target\debug\ghost-link.exe" (
    echo    %YELLOW%[--]%NC%  Backend binary    %DIM%(debug build)%NC%
    set "BACKEND_OK=1"
) else (
    echo    %RED%[XX]%NC%  Backend binary    %DIM%(build with: cargo build --release -p ghost-link)%NC%
)

if exist "third_party\llama.cpp\build\bin\Release\llama-server.exe" (
    echo    %GREEN%[OK]%NC%  llama-server      %DIM%(third_party\llama.cpp\build\bin\Release\)%NC%
    set "LLAMA_OK=1"
) else if exist "third_party\llama.cpp\build\bin\llama-server.exe" (
    echo    %GREEN%[OK]%NC%  llama-server      %DIM%(third_party\llama.cpp\build\bin\)%NC%
    set "LLAMA_OK=1"
) else (
    echo    %YELLOW%[--]%NC%  llama-server      %DIM%(not built; will build on first launch)%NC%
)

set "MODEL_PATH="
for %%f in (models\*.gguf) do (
    if exist "%%f" (
        set "MODEL_PATH=%%f"
    )
)
if defined MODEL_PATH (
    for %%f in ("!MODEL_PATH!") do set "MODEL_SIZE=%%~zf"
    if defined MODEL_SIZE (
        set /a "MODEL_SIZE_MB=!MODEL_SIZE! / 1048576"
        2>nul
    )
    echo    %GREEN%[OK]%NC%  Model             %DIM%!MODEL_PATH! (!MODEL_SIZE_MB! MB)%NC%
    set "MODEL_OK=1"
) else (
    echo    %YELLOW%[--]%NC%  Model             %DIM%(none found; launch.bat will download one)%NC%
)

if exist "ghostlink_gui_modern\package.json" (
    echo    %GREEN%[OK]%NC%  React GUI         %DIM%(ghostlink_gui_modern/)%NC%
    set "GUI_OK=1"
) else (
    echo    %RED%[XX]%NC%  React GUI         %DIM%(ghostlink_gui_modern/ not found)%NC%
)

echo.

REM ──────────────── LAUNCH SUMMARY ────────────────
echo %GREEN%%BOLD%  ✓ Ready to Launch%NC%
echo.
echo    %WHITE%Backend%NC%       http://127.0.0.1:8003
echo    %WHITE%Frontend%NC%      http://127.0.0.1:5173
echo    %WHITE%Inference%NC%     http://127.0.0.1:8080 %DIM%(llama-server)%NC%
echo.

REM Detect Python virtualenv
set "PY_ACTIVE="
if exist ".venv\Scripts\python.exe" set "PY_ACTIVE=.venv"

echo %GRAY%  Prerequisites: Rust, Node.js, CMake%NC%
if defined PY_ACTIVE (
    echo %GRAY%  Python venv:     %PY_ACTIVE%%NC%
)
echo.

echo %CYAN%  Starting services...%NC%
echo.

call "%~dp0launch-complete.bat" %*
exit /b %errorlevel%
