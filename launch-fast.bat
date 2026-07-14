@echo off
REM Ghostlink Fast Launch - uses pre-built binary, skips cargo build
setlocal enabledelayedexpansion
cd /d "%~dp0"

set "BACKEND_HOST=127.0.0.1"
set "BACKEND_PORT=8003"
set "GUI_PORT=5173"
set "LLAMA_PORT=8080"
set "LLAMA_NGL=%GHOSTLINK_LLAMA_NGL%"
if "%LLAMA_NGL%"=="" set "LLAMA_NGL=-1"

set "MODEL_DIR=.\models"
set "MODEL_FILE=%MODEL_DIR%\stories15M-q4_0.gguf"
set "MODEL_URL=https://huggingface.co/ggml-org/models/resolve/main/tinyllamas/stories15M-q4_0.gguf"

REM Check if ports are already in use
echo Checking port availability...
for %%P in (%BACKEND_PORT% %GUI_PORT% %LLAMA_PORT%) do (
    netstat -ano | findstr "%%P" | findstr "LISTENING" >nul 2>&1
    if not errorlevel 1 (
        echo [ERROR] Port %%P is already in use. Please stop the existing service or use a different port.
        pause
        exit /b 1
    )
)
echo [OK] All ports are available

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

REM Determine llama-server binary
set "LLAMA_SERVER=third_party\llama.cpp\build\bin\Release\llama-server.exe"
if not exist "%LLAMA_SERVER%" set "LLAMA_SERVER=third_party\llama.cpp\build\bin\llama-server.exe"

set "GHOSTLINK_NATIVE_ENGINE=simulated"

if exist "%LLAMA_SERVER%" (
    if not exist "%MODEL_DIR%" mkdir "%MODEL_DIR%" 2>nul
    if not exist "%MODEL_FILE%" (
        echo Downloading model...
        curl -L --fail -o "%MODEL_FILE%" "%MODEL_URL%" >nul 2>&1
    )
    echo Starting llama-server on port %LLAMA_PORT%...
    start "llama-server" cmd /k ""%LLAMA_SERVER%" -m "%MODEL_FILE%" --host 127.0.0.1 --port %LLAMA_PORT% -ngl %LLAMA_NGL%"

    echo Waiting for llama-server...
    :WAIT_LLAMA
    curl -sf http://127.0.0.1:%LLAMA_PORT%/health >nul 2>&1
    if errorlevel 1 (
        ping -n 2 127.0.0.1 >nul
        goto WAIT_LLAMA
    )
    echo [OK] llama-server is healthy
    set "GHOSTLINK_NATIVE_ENGINE=llama_server"
) else (
    echo llama-server not built. Run launch.bat first or build manually.
)

set "GHOSTLINK_INFERENCE_BACKEND=native"
set "GHOSTLINK_LLAMA_SERVER_URL=http://127.0.0.1:%LLAMA_PORT%/completion"

REM Set environment variables BEFORE starting the API server so they are inherited
set GHOSTLINK_INFERENCE_BACKEND=%GHOSTLINK_INFERENCE_BACKEND%
set GHOSTLINK_NATIVE_ENGINE=%GHOSTLINK_NATIVE_ENGINE%
set GHOSTLINK_LLAMA_SERVER_URL=%GHOSTLINK_LLAMA_SERVER_URL%

echo Starting Ghostlink API on port %BACKEND_PORT%...
echo   Inference Backend: %GHOSTLINK_INFERENCE_BACKEND%
echo   Native Engine: %GHOSTLINK_NATIVE_ENGINE%
echo   Llama Server URL: %GHOSTLINK_LLAMA_SERVER_URL%
start "Ghostlink API" cmd /k ""%BINARY%" serve %BACKEND_HOST% %BACKEND_PORT%"

echo Waiting for Ghostlink API...
:WAIT_API
curl -sf http://%BACKEND_HOST%:%BACKEND_PORT%/health >nul 2>&1
if errorlevel 1 (
    ping -n 2 127.0.0.1 >nul
    goto WAIT_API
)
echo [OK] Ghostlink API is healthy

if exist "ghostlink_gui_modern\package.json" (
    echo Starting GUI on port %GUI_PORT%...
    pushd ghostlink_gui_modern
    if not exist "node_modules" npm install --legacy-peer-deps >nul 2>&1
    start "Ghostlink GUI" cmd /k "npm run dev -- --host 127.0.0.1 --port %GUI_PORT%"
    popd
)

echo.
echo [OK] All services starting. Open http://127.0.0.1:%GUI_PORT% in your browser.
echo.

start "" "http://127.0.0.1:%GUI_PORT%"

pause
