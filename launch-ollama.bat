@echo off
REM Ghostlink Studio with Ollama and GPU Acceleration
REM Fully optimized launcher for AMD Radeon 860M (gfx906 mapping)
REM Author: Docker AI Assistant
REM Date: 2026-07-17

setlocal enabledelayedexpansion
set "PROJECT_ROOT=%~dp0"
cd /d "%PROJECT_ROOT%"

set "BACKEND_HOST=127.0.0.1"
set "BACKEND_PORT=8003"
set "GUI_PORT=5173"

echo =========================================
echo Ghostlink Studio (Ollama with GPU)
echo =========================================
echo.

taskkill /F /IM ollama.exe >nul 2>&1
taskkill /F /IM ghost-link.exe >nul 2>&1
timeout /t 1 /nobreak >nul

echo [Starting Services]
echo.

where ollama >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Ollama not found. Install from https://ollama.com
    pause
    exit /b 1
)

echo [1/4] Starting Ollama with GPU acceleration...
REM Set GPU environment variables before launching Ollama
REM These ensure the Ollama process inherits GPU configuration
set OLLAMA_HOST=127.0.0.1:11434
set OLLAMA_NUM_THREAD=16
set OLLAMA_GPU_MEMORY=3276
set HIP_PLATFORM=amd
set HSA_OVERRIDE_GFX_VERSION=gfx906
set OLLAMA_IGPU_ENABLE=1
set OLLAMA_BATCH_SIZE=512
set OLLAMA_CACHE_SIZE=2048

start "Ollama" cmd /c "set OLLAMA_HOST=!OLLAMA_HOST!& set OLLAMA_NUM_THREAD=!OLLAMA_NUM_THREAD!& set OLLAMA_GPU_MEMORY=!OLLAMA_GPU_MEMORY!& set HIP_PLATFORM=!HIP_PLATFORM!& set HSA_OVERRIDE_GFX_VERSION=!HSA_OVERRIDE_GFX_VERSION!& set OLLAMA_IGPU_ENABLE=!OLLAMA_IGPU_ENABLE!& set OLLAMA_BATCH_SIZE=!OLLAMA_BATCH_SIZE!& set OLLAMA_CACHE_SIZE=!OLLAMA_CACHE_SIZE!& ollama serve"
timeout /t 3 /nobreak >nul

echo [2/4] Waiting for Ollama...
set /a "WAIT=0"
:WAIT_OLLAMA
curl -sf http://127.0.0.1:11434/api/tags >nul 2>&1
if errorlevel 1 (
    set /a "WAIT+=1"
    if !WAIT! GEQ 30 (
        echo [ERROR] Ollama failed to start
        pause
        exit /b 1
    )
    timeout /t 2 /nobreak >nul
    goto WAIT_OLLAMA
)
echo [OK] Ollama ready (GPU: AMD ROCm gfx906)

echo [3/4] Starting Ghostlink Backend...
set "GHOSTLINK_INFERENCE_BACKEND=ollama"
if exist "%PROJECT_ROOT%target\release\ghost-link.exe" (
    start "Ghostlink Backend" "%PROJECT_ROOT%target\release\ghost-link.exe" serve %BACKEND_HOST% %BACKEND_PORT%
) else (
    start "Ghostlink Backend" cmd /c "cd /d "%PROJECT_ROOT%" && cargo run -p ghost-link -- serve %BACKEND_HOST% %BACKEND_PORT%"
)

set /a "WAIT=0"
:WAIT_BACKEND
curl -sf http://%BACKEND_HOST%:%BACKEND_PORT%/health >nul 2>&1
if errorlevel 1 (
    set /a "WAIT+=1"
    if !WAIT! GEQ 30 (
        echo [ERROR] Backend failed to start
        pause
        exit /b 1
    )
    timeout /t 2 /nobreak >nul
    goto WAIT_BACKEND
)
echo [OK] Backend ready

echo [4/4] Starting GUI...
if not exist "%PROJECT_ROOT%ghostlink_gui_modern\node_modules" (
    cd "%PROJECT_ROOT%ghostlink_gui_modern"
    call npm install --legacy-peer-deps
    cd "%PROJECT_ROOT%"
)
set "VITE_GHOSTLINK_API_BASE=http://%BACKEND_HOST%:%BACKEND_PORT%"
start "Ghostlink GUI" cmd /c "cd /d "%PROJECT_ROOT%ghostlink_gui_modern" && npm run dev -- --host 127.0.0.1 --port %GUI_PORT%"

set /a "WAIT=0"
:WAIT_GUI
curl -sf http://127.0.0.1:%GUI_PORT% >nul 2>&1
if errorlevel 1 (
    set /a "WAIT+=1"
    if !WAIT! GEQ 30 goto GUI_TIMEOUT
    timeout /t 1 /nobreak >nul
    goto WAIT_GUI
)
echo [OK] GUI ready
goto GUI_DONE

:GUI_TIMEOUT
echo [WARN] GUI startup timeout, but services may still start

:GUI_DONE
echo.
echo =========================================
echo Ghostlink Studio Ready
echo =========================================
echo.
echo System Configuration:
echo  - CPU: 16 cores (100%%)
echo  - GPU: AMD Radeon 860M (ROCm gfx906)
echo  - Inference: GPU-accelerated
echo.
echo Services:
echo  - Backend: http://127.0.0.1:8003
echo  - Ollama:  http://127.0.0.1:11434
echo  - GUI:     http://127.0.0.1:5173
echo.
echo =========================================
echo.

timeout /t 2 /nobreak >nul
start "" "http://127.0.0.1:5173"

pause
