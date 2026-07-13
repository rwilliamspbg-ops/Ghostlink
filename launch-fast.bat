@echo off
REM Ghostlink Fast Launch - uses pre-built binary, skips cargo build
setlocal enabledelayedexpansion
cd /d "%~dp0"

set "BACKEND_HOST=127.0.0.1"
set "BACKEND_PORT=8003"
set "GUI_PORT=5173"
set "LLAMA_PORT=8080"

if not exist "target\release\ghost-link.exe" (
    if exist "target\debug\ghost-link.exe" (
        set "BINARY=target\debug\ghost-link.exe"
    ) else (
        echo Binary not found. Run: cargo build --release -p ghost-link
        pause
        exit /b 1
    )
) else (
    set "BINARY=target\release\ghost-link.exe"
)

echo Ghostlink Binary: %BINARY%
echo.
echo Starting services...

REM Start llama-server if available
set "LLAMA_SERVER=third_party\llama.cpp\build\bin\Release\llama-server.exe"
if not exist "%LLAMA_SERVER%" set "LLAMA_SERVER=third_party\llama.cpp\build\bin\llama-server.exe"
set "MODEL=models\stories15M-q4_0.gguf"

if exist "%LLAMA_SERVER%" (
    echo Starting llama-server on port %LLAMA_PORT%...
    start "llama-server" cmd /k ""%LLAMA_SERVER%" -m "%MODEL%" --host 127.0.0.1 --port %LLAMA_PORT% -ngl -1"
) else (
    echo llama-server not built. Run launch.bat first or build manually.
)

REM Start ghostlink API using pre-built binary
echo Starting Ghostlink API on port %BACKEND_PORT%...
start "Ghostlink API" cmd /k ""%BINARY%" serve %BACKEND_HOST% %BACKEND_PORT%"

REM Start GUI
if exist "ghostlink_gui_modern\package.json" (
    echo Starting GUI on port %GUI_PORT%...
    pushd ghostlink_gui_modern
    if not exist "node_modules" npm install --legacy-peer-deps >nul 2>&1
    start "Ghostlink GUI" cmd /k "npm run dev -- --host 127.0.0.1 --port %GUI_PORT%"
    popd
)

echo.
echo All services starting. Open http://127.0.0.1:%GUI_PORT% in your browser.
echo.
pause
