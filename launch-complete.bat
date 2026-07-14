@echo off
REM Ghostlink Studio - Complete Launch Script (Windows)
REM Native llama-server + backend + GUI

setlocal enabledelayedexpansion

set "BACKEND_HOST=127.0.0.1"
set "BACKEND_PORT=8003"
set "GUI_PORT=5173"
set "LLAMA_PORT=8080"
set "PROJECT_ROOT=%~dp0"
set "LLAMA_NGL=%GHOSTLINK_LLAMA_NGL%"
if "%LLAMA_NGL%"=="" set "LLAMA_NGL=-1"

set "MODEL_DIR=%PROJECT_ROOT%models"
set "MODEL_FILE=%MODEL_DIR%\stories15M-q4_0.gguf"
set "MODEL_URL=https://huggingface.co/ggml-org/models/resolve/main/tinyllamas/stories15M-q4_0.gguf"

if exist "%PROJECT_ROOT%settings.json" (
    for /f "usebackq tokens=*" %%a in (`powershell -NoProfile -Command "try { $s = Get-Content '%PROJECT_ROOT%settings.json' -Raw | ConvertFrom-Json; if ($s.model_path) { Write-Output $s.model_path } } catch {}"`) do (
        set "MODEL_FILE=%%a"
    )
)

set "LLAMA_SERVER=%PROJECT_ROOT%third_party\llama.cpp\build\bin\Release\llama-server.exe"
set "LLAMA_SERVER_ALT=%PROJECT_ROOT%third_party\llama.cpp\build\bin\llama-server.exe"

REM Auto-detect GPU
set "GPU_VENDOR="
where nvidia-smi >nul 2>&1
if not errorlevel 1 (
    for /f "tokens=*" %%a in ('nvidia-smi --query-gpu=name --format=csv,noheader 2^>nul') do set "GPU_NAME=%%a"
    if defined GPU_NAME (
        echo [INFO] NVIDIA GPU: !GPU_NAME!
        set "GPU_VENDOR=nvidia"
    )
)
if not defined GPU_VENDOR (
    where rocm-smi >nul 2>&1
    if not errorlevel 1 (
        echo [INFO] AMD ROCm detected
        set "GPU_VENDOR=amd"
    )
)
if not defined GPU_VENDOR (
    for /f "skip=1 tokens=2 delims=," %%a in ('wmic path Win32_VideoController get Name /format:csv 2^>nul') do (
        echo %%a | findstr /i "amd radeon advanced.micro" >nul
        if not errorlevel 1 (
            echo [INFO] AMD GPU: %%a
            set "GPU_VENDOR=amd"
        )
    )
)
if not defined GPU_VENDOR (
    if "%LLAMA_NGL%"=="-1" (
        echo [INFO] No GPU detected - using CPU mode (ngl=0)
        set "LLAMA_NGL=0"
    )
)

echo.
echo ================================================================================
echo   Ghostlink Studio - Complete Launch
echo ================================================================================

cargo --version >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Rust/Cargo not found. Install from https://rustup.rs/
    exit /b 1
)
echo [OK] Cargo verified

node --version >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Node.js not found. Install from https://nodejs.org/
    exit /b 1
)
echo [OK] Node.js verified

echo.

REM Ensure model directory exists
if not exist "%MODEL_DIR%" mkdir "%MODEL_DIR%" 2>nul

REM Download model if needed
if not exist "!MODEL_FILE!" (
    if "!MODEL_FILE!"=="%MODEL_DIR%\stories15M-q4_0.gguf" (
        echo [INFO] Downloading model (stories15M-q4_0.gguf ~15MB)...
        curl -L --fail -o "!MODEL_FILE!" "%MODEL_URL%" >nul 2>&1
        if errorlevel 1 (
            echo [ERROR] Model download failed
            pause
            exit /b 1
        )
        echo [OK] Model downloaded
    ) else (
        echo [WARN] Model not found: !MODEL_FILE!
        echo [WARN] Using default model...
        set "MODEL_FILE=%MODEL_DIR%\stories15M-q4_0.gguf"
        if not exist "!MODEL_FILE!" (
            curl -L --fail -o "!MODEL_FILE!" "%MODEL_URL%" >nul 2>&1
        )
    )
)

REM Determine llama-server binary
set "ACTUAL_LLAMA_SERVER="
if exist "%LLAMA_SERVER%" set "ACTUAL_LLAMA_SERVER=%LLAMA_SERVER%"
if exist "%LLAMA_SERVER_ALT%" if "!ACTUAL_LLAMA_SERVER!"=="" set "ACTUAL_LLAMA_SERVER=%LLAMA_SERVER_ALT%"

echo.
echo Starting services...
echo.

REM Start llama-server if binary exists
set "GHOSTLINK_NATIVE_ENGINE=simulated"
if defined ACTUAL_LLAMA_SERVER (
    echo [INFO] Starting llama-server on http://127.0.0.1:%LLAMA_PORT% ...
    start "llama-server" cmd /k ""!ACTUAL_LLAMA_SERVER!" -m "!MODEL_FILE!" --host 127.0.0.1 --port %LLAMA_PORT% -ngl !LLAMA_NGL!"

    echo [INFO] Waiting for llama-server health check...
    :WAIT_LLAMA
    curl -sf http://127.0.0.1:%LLAMA_PORT%/health >nul 2>&1
    if errorlevel 1 (
        ping -n 2 127.0.0.1 >nul
        goto WAIT_LLAMA
    )
    echo [OK] llama-server is healthy
    set "GHOSTLINK_NATIVE_ENGINE=llama_server"
) else (
    echo [WARN] llama-server binary not found. Using simulated native mode.
    echo [WARN] Run launch.bat first to build llama-server.
)

set "GHOSTLINK_INFERENCE_BACKEND=native"
set "GHOSTLINK_LLAMA_SERVER_URL=http://127.0.0.1:%LLAMA_PORT%/completion"

REM Start backend
echo [INFO] Starting Backend API on http://%BACKEND_HOST%:%BACKEND_PORT%

set "GHOSTLINK_BINARY="
if exist "%PROJECT_ROOT%target\release\ghost-link.exe" set "GHOSTLINK_BINARY=%PROJECT_ROOT%target\release\ghost-link.exe"
if exist "%PROJECT_ROOT%target\debug\ghost-link.exe" if "!GHOSTLINK_BINARY!"=="" set "GHOSTLINK_BINARY=%PROJECT_ROOT%target\debug\ghost-link.exe"

if defined GHOSTLINK_BINARY (
    start "Ghostlink Backend API" cmd /k "cd /d "%PROJECT_ROOT%" && set GHOSTLINK_INFERENCE_BACKEND=%GHOSTLINK_INFERENCE_BACKEND% && set GHOSTLINK_NATIVE_ENGINE=%GHOSTLINK_NATIVE_ENGINE% && set GHOSTLINK_LLAMA_SERVER_URL=%GHOSTLINK_LLAMA_SERVER_URL% && "!GHOSTLINK_BINARY!" serve %BACKEND_HOST% %BACKEND_PORT%"
) else (
    echo [INFO] No pre-built binary, building with cargo...
    start "Ghostlink Backend API" cmd /k "cd /d "%PROJECT_ROOT%" && set GHOSTLINK_INFERENCE_BACKEND=%GHOSTLINK_INFERENCE_BACKEND% && set GHOSTLINK_NATIVE_ENGINE=%GHOSTLINK_NATIVE_ENGINE% && set GHOSTLINK_LLAMA_SERVER_URL=%GHOSTLINK_LLAMA_SERVER_URL% && cargo run -p ghost-link -- serve %BACKEND_HOST% %BACKEND_PORT%"
)

echo [INFO] Waiting for Ghostlink API health check...
:WAIT_API
curl -sf http://%BACKEND_HOST%:%BACKEND_PORT%/health >nul 2>&1
if errorlevel 1 (
    ping -n 2 127.0.0.1 >nul
    goto WAIT_API
)
echo [OK] Ghostlink API is healthy

REM Start GUI
echo [INFO] Starting GUI Frontend on http://127.0.0.1:%GUI_PORT%
cd "%PROJECT_ROOT%ghostlink_gui_modern"
if not exist "node_modules" (
    echo [INFO] Installing npm dependencies...
    call npm install --legacy-peer-deps >nul 2>&1
    if errorlevel 1 (
        echo [ERROR] npm install failed
        pause
        exit /b 1
    )
    echo [OK] Dependencies installed
)
start "Ghostlink Studio GUI" cmd /k "npm run dev -- --host 127.0.0.1 --port %GUI_PORT%"
cd "%PROJECT_ROOT%"

echo [INFO] Waiting for Vite dev server...
:WAIT_GUI
curl -sf http://127.0.0.1:%GUI_PORT% >nul 2>&1
if errorlevel 1 (
    ping -n 2 127.0.0.1 >nul
    goto WAIT_GUI
)
echo [OK] React Frontend is healthy

echo.
echo ================================================================================
echo   Ghostlink Studio is Ready
echo ================================================================================
echo.
echo   Web Interface:     http://127.0.0.1:%GUI_PORT%
echo   API Server:        http://%BACKEND_HOST%:%BACKEND_PORT%
echo   Native Inference:  http://127.0.0.1:%LLAMA_PORT% (llama-server)
echo.
echo   1. Open http://127.0.0.1:%GUI_PORT% in your browser
echo   2. Go to Models tab - Select a model
echo   3. Switch to Chat tab - Start talking!
echo.
echo ================================================================================
echo.

start "" "http://127.0.0.1:%GUI_PORT%"

pause
