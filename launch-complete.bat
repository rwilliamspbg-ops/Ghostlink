@echo off
REM
REM Ghostlink Studio - Complete Launch Script (Windows)
REM Starts all services: Ollama, Backend API, GUI Frontend, and Runtime Detection
REM

setlocal enabledelayedexpansion

set "BACKEND_HOST=127.0.0.1"
set "BACKEND_PORT=8003"
set "GUI_PORT=5173"
set "PROJECT_ROOT=%~dp0"

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
echo Configuring Ollama backend...
python use_ollama.py
if errorlevel 1 (
    echo [WARN] Failed to apply Ollama configuration. Continuing with default backend.
) else (
    echo [✓] Ollama configuration applied.
)

echo.
echo Starting services...
echo.

REM Start Ollama service in a new window
echo [INFO] Starting Ollama Service...
start "Ollama" cmd /k "ollama serve"
timeout /t 3 /nobreak >nul
echo [✓] Ollama startup command issued.

REM Start backend in new window
echo [INFO] Starting Backend API on http://%BACKEND_HOST%:%BACKEND_PORT%
start "Ghostlink Backend API" cmd /k "cd /d \"%PROJECT_ROOT%crates\\ghost-link\" && cargo run --release -- serve %BACKEND_HOST% %BACKEND_PORT%"
timeout /t 3 /nobreak >nul

REM Start GUI in new window
echo [INFO] Starting GUI Frontend on http://localhost:%GUI_PORT%
start "Ghostlink Studio GUI" cmd /k "cd /d \"%PROJECT_ROOT%ghostlink_gui_modern\" && (if not exist node_modules npm install) && npm run dev"
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
echo   URL: http://localhost:%GUI_PORT%
echo   Status: ✓ Running in new window
echo.
echo Test Commands:
echo   Runtime:   curl http://%BACKEND_HOST%:%BACKEND_PORT%/api/runtime/detect
echo   Models:    curl "http://%BACKEND_HOST%:%BACKEND_PORT%/api/runtime/models?runtime=cpu"
echo   Recommend: curl "http://%BACKEND_HOST%:%BACKEND_PORT%/api/runtime/recommend?memory_gb=8"
echo Open http://localhost:%GUI_PORT% in your browser
echo ════════════════════════════════════════════════════
echo.

pause
