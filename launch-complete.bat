@echo off
REM Ghostlink Studio - Complete Launch Script (Windows)
REM Native llama-server + backend + GUI

setlocal enabledelayedexpansion

set "BACKEND_HOST=127.0.0.1"
set "BACKEND_PORT=8003"
set "GUI_PORT=5173"
set "PROJECT_ROOT=%~dp0"
set "LLAMA_SERVER_BIN=%PROJECT_ROOT%third_party\llama.cpp\build\bin\llama-server.exe"
set "LLAMA_SERVER_BIN_ALT=%PROJECT_ROOT%third_party\llama.cpp\build\bin\llama-server"
set "LLAMA_MODEL=%PROJECT_ROOT%tmp\models\model.gguf"
set "LLAMA_SERVER_URL=http://127.0.0.1:8080/completion"

echo.
echo ════════════════════════════════════════════════════
echo   Ghostlink Studio - Complete Launch
echo ════════════════════════════════════════════════════

REM Check for Cargo
cargo --version >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Rust/Cargo not found. Install from https://rustup.rs/
    exit /b 1
)
echo [✓] Cargo verified

REM Check for Node
node --version >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Node.js not found. Install from https://nodejs.org/
    exit /b 1
)
echo [✓] Node.js verified

echo.
echo Building backend...
cd "%PROJECT_ROOT%crates\ghost-link"
call cargo build --release
if errorlevel 1 (
    echo [ERROR] Backend build failed
    exit /b 1
)
echo [✓] Backend built
cd "%PROJECT_ROOT%"

echo.
echo Starting services...
echo.

if exist "%LLAMA_SERVER_BIN%" (
    echo [INFO] Starting llama-server on http://127.0.0.1:8080 ...
    if not exist "%LLAMA_MODEL%" (
        echo [WARN] Model not found at %LLAMA_MODEL%
        echo [WARN] Falling back to simulated native mode.
        set "GHOSTLINK_NATIVE_ENGINE=simulated"
    ) else (
        start "llama-server" cmd /k ""%LLAMA_SERVER_BIN%" -m "%LLAMA_MODEL%" --host 127.0.0.1 --port 8080 -ngl 0"
        timeout /t 3 /nobreak >nul
        set "GHOSTLINK_NATIVE_ENGINE=llama_server"
    )
)
if not defined GHOSTLINK_NATIVE_ENGINE if exist "%LLAMA_SERVER_BIN_ALT%" (
    echo [INFO] Starting llama-server on http://127.0.0.1:8080 ...
    if not exist "%LLAMA_MODEL%" (
        echo [WARN] Model not found at %LLAMA_MODEL%
        echo [WARN] Falling back to simulated native mode.
        set "GHOSTLINK_NATIVE_ENGINE=simulated"
    ) else (
        start "llama-server" cmd /k ""%LLAMA_SERVER_BIN_ALT%" -m "%LLAMA_MODEL%" --host 127.0.0.1 --port 8080 -ngl 0"
        timeout /t 3 /nobreak >nul
        set "GHOSTLINK_NATIVE_ENGINE=llama_server"
    )
)
if not defined GHOSTLINK_NATIVE_ENGINE (
    echo [WARN] llama-server binary not found at %LLAMA_SERVER_BIN%
    echo [WARN] Backend will run with simulated native mode unless env overrides are provided.
    set "GHOSTLINK_NATIVE_ENGINE=simulated"
)
set "GHOSTLINK_INFERENCE_BACKEND=native"
set "GHOSTLINK_LLAMA_SERVER_URL=%LLAMA_SERVER_URL%"

REM Start backend in new window
echo [INFO] Starting Backend API on http://%BACKEND_HOST%:%BACKEND_PORT%
start "Ghostlink Backend API" cmd /k "cd /d "%PROJECT_ROOT%" && set GHOSTLINK_INFERENCE_BACKEND=%GHOSTLINK_INFERENCE_BACKEND% && set GHOSTLINK_NATIVE_ENGINE=%GHOSTLINK_NATIVE_ENGINE% && set GHOSTLINK_LLAMA_SERVER_URL=%GHOSTLINK_LLAMA_SERVER_URL% && cargo run -p ghost-link -- serve %BACKEND_HOST% %BACKEND_PORT%"
timeout /t 3 /nobreak >nul

REM Start GUI in new window
echo [INFO] Starting GUI Frontend on http://127.0.0.1:%GUI_PORT%
start "Ghostlink Studio GUI" cmd /k "cd /d \"%PROJECT_ROOT%ghostlink_gui_modern\" && (if not exist node_modules npm install --legacy-peer-deps) && npm run dev -- --host 127.0.0.1 --port %GUI_PORT%"
timeout /t 3 /nobreak >nul

echo.
echo ════════════════════════════════════════════════════
echo   Services Starting
echo ════════════════════════════════════════════════════
echo.
echo Backend API:
echo   URL: http://%BACKEND_HOST%:%BACKEND_PORT%
echo   Status: ✓ Running in new window
echo.
echo Runtime Detection Endpoints:
echo   Detect:    GET /api/runtime/detect
echo   Models:    GET /api/runtime/models?runtime=cpu
echo   Recommend: GET /api/runtime/recommend?memory_gb=16
echo.
echo GUI Frontend:
echo   URL: http://127.0.0.1:%GUI_PORT%
echo   Status: ✓ Running in new window
echo.
echo Test Commands:
echo   Runtime:   curl http://%BACKEND_HOST%:%BACKEND_PORT%/api/runtime/detect
echo   Models:    curl "http://%BACKEND_HOST%:%BACKEND_PORT%/api/runtime/models?runtime=cpu"
echo   Recommend: curl "http://%BACKEND_HOST%:%BACKEND_PORT%/api/runtime/recommend?memory_gb=8"
echo   Native:    curl -X POST http://%BACKEND_HOST%:%BACKEND_PORT%/api/inference/chat -H "content-type: application/json" -d "{\"message\":\"hello\"}"
echo Open http://127.0.0.1:%GUI_PORT% in your browser
echo ════════════════════════════════════════════════════
echo.

pause
