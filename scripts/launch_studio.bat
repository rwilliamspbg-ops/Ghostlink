@echo off
REM Ghostlink Studio - Launch Script for Windows

setlocal enabledelayedexpansion

set "ROOT_DIR=%~dp0.."
cd /d "%ROOT_DIR%"

echo [Ghostlink] Starting Ghostlink Studio initialization...

REM Check for --check flag
if "%1"=="--check" (
    echo [Ghostlink] Running preflight checks...

    REM Check Ollama
    curl -s http://127.0.0.1:11434/api/health >nul 2>&1
    if !errorlevel! equ 0 (
        echo [Ghostlink]   [OK] Ollama running on port 11434
    ) else (
        echo [Ghostlink]   [ERROR] Ollama not running
        exit /b 1
    )

    REM Check neural-chat model
    ollama list | findstr neural-chat >nul
    if !errorlevel! equ 0 (
        echo [Ghostlink]   [OK] neural-chat model available
    ) else (
        echo [Ghostlink]   [WARN] neural-chat model not loaded
    )

    echo [Ghostlink]   [OK] Backend will run on port 8003
    echo [Ghostlink]   [OK] GUI proxy will run on port 9999
    echo [Ghostlink] Preflight completed successfully
    exit /b 0
)

REM Build
echo [Ghostlink] Building Ghostlink backend...
cargo build --release -p ghost-link 2>&1 | findstr /c:"Finished" /c:"error"
if !errorlevel! neq 0 (
    echo [Ghostlink] Build failed
    exit /b 1
)

REM Start services
echo [Ghostlink] Starting services...

REM Check if Ollama is running
curl -s http://127.0.0.1:11434/api/health >nul 2>&1
if !errorlevel! neq 0 (
    echo [Ghostlink] Starting Ollama...
    start /b ollama serve
    timeout /t 3 /nobreak
)

REM Start backend
echo [Ghostlink] Starting backend on port 8003...
start /b "Ghostlink Backend" target\release\ghost-link.exe serve
timeout /t 2 /nobreak

REM Start proxy
echo [Ghostlink] Starting LLM proxy on port 9999...
start /b "Ghostlink Proxy" python3 real_llm_proxy.py
timeout /t 2 /nobreak

REM Start GUI
echo [Ghostlink] Launching GUI...
python3 ghostlink_gui.py --backend-url http://127.0.0.1:9999
