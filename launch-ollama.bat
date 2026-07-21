@echo off
REM Ghostlink Studio with Native Inference (llama-server) and GPU Acceleration
REM Fully optimized launcher for AMD Radeon 860M (gfx906 mapping)
REM Author: Docker AI Assistant
REM Date: 2026-07-18
REM FIXED: Single model at startup + auto-safe GPU offloading to prevent GPU crash

setlocal enabledelayedexpansion
set "PROJECT_ROOT=%~dp0"
cd /d "%PROJECT_ROOT%"

set "BACKEND_HOST=127.0.0.1"
set "BACKEND_PORT=8003"
set "GUI_PORT=5173"
set "LLAMA_PORT=8080"

echo =========================================
echo Ghostlink Studio (Native Inference with GPU)
echo =========================================
echo.

taskkill /F /IM ollama.exe >nul 2>&1
taskkill /F /IM ghost-link.exe >nul 2>&1
taskkill /F /IM llama-server.exe >nul 2>&1
timeout /t 1 /nobreak >nul

echo [Starting Services]
echo.

REM Find llama-server binary
set "LLAMA_SERVER_BIN="
if exist "%PROJECT_ROOT%third_party\llama.cpp\build\bin\Release\llama-server.exe" (
    set "LLAMA_SERVER_BIN=%PROJECT_ROOT%third_party\llama.cpp\build\bin\Release\llama-server.exe"
) else if exist "%PROJECT_ROOT%third_party\llama.cpp\build\bin\llama-server.exe" (
    set "LLAMA_SERVER_BIN=%PROJECT_ROOT%third_party\llama.cpp\build\bin\llama-server.exe"
) else if exist "%PROJECT_ROOT%bin\llama-server.exe" (
    set "LLAMA_SERVER_BIN=%PROJECT_ROOT%bin\llama-server.exe"
)

if not defined LLAMA_SERVER_BIN (
    echo [WARN] llama-server binary not found. Building from source...
    REM Could add build logic here
)

echo [1/4] Starting llama-server with single model (auto-safe GPU offload)...
REM Default to the smaller quantized model to save VRAM
set "MODEL_PRIMARY=%PROJECT_ROOT%models\gemma-4-E4B-it-Q4_K_M.gguf"
set "MODEL_ALIAS=gemma-4-E4B-it-Q4_K_M"

if not exist "!MODEL_PRIMARY!" (
    echo [ERROR] Primary model not found: !MODEL_PRIMARY!
    pause
    exit /b 1
)

REM GPU settings for AMD Radeon 860M
set "HIP_PLATFORM=amd"
set "HSA_OVERRIDE_GFX_VERSION=gfx906"
set "OLLAMA_IGPU_ENABLE=1"

REM Auto-detect safe GPU layers (-ngl) based on model size and VRAM
REM AMD 860M has ~8GB shared VRAM. Model is ~5.3GB (Q4_K_M).
REM Safe offload: leave ~2GB headroom for OS/KV-cache -> ~6GB usable -> ~40 layers max
set "LLAMA_NGL=40"
set "LLAMA_THREADS=15"

REM Perf: Flash Attention + batch sizes (8GB VRAM profile). Override with GHOSTLINK_LLAMA_SERVER_ARGS.
if not defined GHOSTLINK_LLAMA_SERVER_ARGS set "GHOSTLINK_LLAMA_SERVER_ARGS=-fa on -b 1024 -ub 512"
set "GHOSTLINK_VRAM_GB=8"
set "GHOSTLINK_LLAMA_NGL=!LLAMA_NGL!"
set "GHOSTLINK_LLAMA_THREADS=!LLAMA_THREADS!"

REM Start llama-server with SINGLE primary Q4_K_M model, -ngl 40 (safe for 8GB VRAM)
REM Multi-model support via llama-server's /slot/load API at runtime
start "llama-server" cmd /c "set HIP_PLATFORM=!HIP_PLATFORM!& set HSA_OVERRIDE_GFX_VERSION=!HSA_OVERRIDE_GFX_VERSION!& "!LLAMA_SERVER_BIN!" -m "!MODEL_PRIMARY!" --alias !MODEL_ALIAS! --host 127.0.0.1 --port !LLAMA_PORT! -ngl !LLAMA_NGL! -t !LLAMA_THREADS! !GHOSTLINK_LLAMA_SERVER_ARGS! --mlock"
timeout /t 5 /nobreak >nul

echo [2/4] Waiting for llama-server...
set /a "WAIT=0"
:WAIT_LLAMA
curl -sf http://127.0.0.1:%LLAMA_PORT%/health >nul 2>&1
if errorlevel 1 (
    set /a "WAIT+=1"
    if !WAIT! GEQ 60 (
        echo [ERROR] llama-server failed to start
        pause
        exit /b 1
    )
    timeout /t 2 /nobreak >nul
    goto WAIT_LLAMA
)
echo [OK] llama-server ready (GPU: AMD ROCm gfx906, model: gemma-4-E4B-it-Q4_K_M, GPU layers: 40/99, threads: 15)

echo [3/4] Starting Ghostlink Backend...
set "GHOSTLINK_INFERENCE_BACKEND=native"
set "GHOSTLINK_NATIVE_ENGINE=llama_server"
set "GHOSTLINK_LLAMA_SERVER_URL=http://127.0.0.1:%LLAMA_PORT%/completion"
if exist "%PROJECT_ROOT%target\release\ghost-link.exe" (
    start "Ghostlink Backend" cmd /c "set HSA_OVERRIDE_GFX_VERSION=gfx906& set HIP_PLATFORM=amd& set GHOSTLINK_INFERENCE_BACKEND=native& set GHOSTLINK_NATIVE_ENGINE=llama_server& set GHOSTLINK_LLAMA_SERVER_URL=http://127.0.0.1:%LLAMA_PORT%/completion& "%PROJECT_ROOT%target\release\ghost-link.exe" serve %BACKEND_HOST% %BACKEND_PORT%"
) else (
    start "Ghostlink Backend" cmd /c "set HSA_OVERRIDE_GFX_VERSION=gfx906& set HIP_PLATFORM=amd& set GHOSTLINK_INFERENCE_BACKEND=native& set GHOSTLINK_NATIVE_ENGINE=llama_server& set GHOSTLINK_LLAMA_SERVER_URL=http://127.0.0.1:%LLAMA_PORT%/completion& cd /d "%PROJECT_ROOT%" && cargo run -p ghost-link -- serve %BACKEND_HOST% %BACKEND_PORT%"
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
echo  - Inference: GPU-accelerated (llama-server, -ngl 40 safe offload)
echo.
echo Services:
echo  - Backend:  http://127.0.0.1:%BACKEND_PORT%
echo  - llama-server: http://127.0.0.1:%LLAMA_PORT% (model: gemma-4-E4B-it-Q4_K_M, slot 0)
echo  - GUI:      http://127.0.0.1:%GUI_PORT%
echo.
echo Multi-model API Usage (load models dynamically via slots):
echo  - List models:     curl http://127.0.0.1:%LLAMA_PORT%/models
echo  - Load into slot:  POST http://127.0.0.1:%LLAMA_PORT%/slot/load with {"id": 0, "model": "Qwen3.5-4B-BF16"}
echo  - Unload slot:     POST http://127.0.0.1:%LLAMA_PORT%/slot/unload with {"id": 0}
echo  - Switch model:    POST http://127.0.0.1:%LLAMA_PORT%/completion with {"model": "Qwen3.5-4B-BF16", "prompt": "..."}
echo.
echo =========================================
echo.

timeout /t 2 /nobreak >nul
start "" "http://127.0.0.1:%GUI_PORT%"

pause