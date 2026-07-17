@echo off
setlocal enabledelayedexpansion

REM Ghostlink Studio - Complete Native Launch Script (Windows) - FIXED VERSION
REM No Docker required. Auto-detects hardware and configures llama-server optimally.

REM Clear any stale internal state from prior runs
set "GPU_VENDOR="
set "BACKEND="
set "NPU_DETECTED="
set "LLAMA_NGL="

REM ==================== Configuration ====================
set "BACKEND_HOST=127.0.0.1"
set "BACKEND_PORT=8003"
set "GUI_PORT=5173"
set "LLAMA_PORT=8080"
REM PROJECT_ROOT must be set before any variable that derives from it
set "PROJECT_ROOT=%~dp0"
set "MODEL_DIR=%PROJECT_ROOT%models"
set "MODEL_FILE=%MODEL_DIR%\stories15M-q4_0.gguf"
set "MODEL_URL=https://huggingface.co/ggml-org/models/resolve/main/tinyllamas/stories15M-q4_0.gguf"
set "LLAMA_SERVER_EXPECTED=%PROJECT_ROOT%third_party\llama.cpp\build\bin\Release\llama-server.exe"
set "LLAMA_SERVER_ALT_EXPECTED=%PROJECT_ROOT%third_party\llama.cpp\build\bin\llama-server.exe"

REM ==================== Manual Overrides (set via env vars) ====================
if not "%GHOSTLINK_LLAMA_PORT%"=="" set "LLAMA_PORT=%GHOSTLINK_LLAMA_PORT%"
if not "%GHOSTLINK_MODEL_FILE%"=="" set "MODEL_FILE=%GHOSTLINK_MODEL_FILE%"

REM Validate VITE_GHOSTLINK_API_BASE if set
if not "%VITE_GHOSTLINK_API_BASE%"=="" (
    set "VITE_GHOSTLINK_API_BASE=%VITE_GHOSTLINK_API_BASE: =%"
    echo [INFO] VITE_GHOSTLINK_API_BASE: %VITE_GHOSTLINK_API_BASE%
    echo %VITE_GHOSTLINK_API_BASE% | findstr /r "^https?://" >nul
    if errorlevel 1 (
        echo [ERROR] VITE_GHOSTLINK_API_BASE must be a valid http:// or https:// URL
        pause
        exit /b 1
    )
) else (
    set "VITE_GHOSTLINK_API_BASE=http://%BACKEND_HOST%:%BACKEND_PORT%"
    echo [INFO] Using default VITE_GHOSTLINK_API_BASE: !VITE_GHOSTLINK_API_BASE!
)

REM Check for required commands
for %%c in (curl cargo node) do (
    where %%c >nul 2>&1
    if errorlevel 1 (
        echo [ERROR] %%c not found. Please install it.
        echo.
        pause
        exit /b 1
    ) else (
        echo [OK] %%c verified
    )
)

echo.
echo ====== Configuration ======
echo.

set "GPU_COUNT=0"
set "GPU_VENDOR="
set "GPU_NAME="
set "TOTAL_VRAM_MB=0"
set "NPU_DETECTED="
set "BACKEND="
set "CPU_CORES=0"
set "RAM_GB=0"

REM --- CPU Cores ---
for /f %%a in ('powershell -NoProfile -Command "(Get-CimInstance Win32_ComputerSystem).NumberOfLogicalProcessors" 2^>nul') do set "CPU_CORES=%%a"
if "%CPU_CORES%"=="0" set "CPU_CORES=4"
echo   CPU Cores: !CPU_CORES!

REM --- System RAM ---
for /f %%a in ('powershell -NoProfile -Command "[Math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1GB)\" 2^>nul') do set "RAM_GB=%%a"
if "%RAM_GB%"=="0" set "RAM_GB=4"
echo   System RAM: !RAM_GB! GB

REM --- GPU Detection via PowerShell (all GPUs) ---
set "GPU_INDEX=0"
for /f "delims=" %%a in ('powershell -NoProfile -Command "Get-CimInstance Win32_VideoController | Where-Object { $_.Name -notmatch 'Microsoft Basic Display' } | ForEach-Object { $_.Name }" 2^>nul') do (
    set /a "GPU_INDEX+=1"
    set "GPU_NAME_!GPU_INDEX!=%%a"
)
set "GPU_COUNT=%GPU_INDEX%"

REM --- NVIDIA GPU Detection (CUDA) ---
set "NVIDIA_SMI="
where nvidia-smi >nul 2>&1
if not errorlevel 1 set "NVIDIA_SMI=nvidia-smi"
if not defined NVIDIA_SMI if exist "%SystemRoot%\System32\nvidia-smi.exe" set "NVIDIA_SMI=%SystemRoot%\System32\nvidia-smi.exe"
if not defined NVIDIA_SMI if exist "%ProgramFiles%\NVIDIA Corporation\NVSMI\nvidia-smi.exe" set "NVIDIA_SMI=%ProgramFiles%\NVIDIA Corporation\NVSMI\nvidia-smi.exe"
if defined NVIDIA_SMI (
    "!NVIDIA_SMI!" --query-gpu=name --format=csv,noheader > "%TEMP%\ghostlink_gpu.txt" 2>nul
    for /f "usebackq tokens=*" %%a in ("%TEMP%\ghostlink_gpu.txt") do set "NVIDIA_GPU=%%a"
    del "%TEMP%\ghostlink_gpu.txt" 2>nul
    if defined NVIDIA_GPU (
        echo   [CUDA] NVIDIA GPU: !NVIDIA_GPU!
        set "GPU_VENDOR=nvidia"
        set "BACKEND=cuda"
        "!NVIDIA_SMI!" --query-gpu=memory.total --format=csv,noheader,nounits > "%TEMP%\ghostlink_vram.txt" 2>nul
        for /f "usebackq tokens=1 delims=, " %%a in ("%TEMP%\ghostlink_vram.txt") do set "TOTAL_VRAM_MB=%%a"
        del "%TEMP%\ghostlink_vram.txt" 2>nul
        if "!TOTAL_VRAM_MB!"=="0" set "TOTAL_VRAM_MB=4096"
        set /a "VRAM_GB=!TOTAL_VRAM_MB! / 1024" 2>nul
        if "!VRAM_GB!"=="0" set "VRAM_GB=4"
        echo   VRAM: !VRAM_GB! GB (!TOTAL_VRAM_MB! MB)
    )
)

REM --- NVIDIA GPU via WMI fallback (driver present but nvidia-smi unavailable) ---
if not defined GPU_VENDOR (
    for /f "delims=" %%a in ('powershell -NoProfile -Command "Get-CimInstance Win32_VideoController | Where-Object { $_.Name -match 'nvidia|geforce|rtx|gtx|quadro' } | Select-Object -First 1 | ForEach-Object { $_.Name }" 2^>nul') do (
        echo   [Vulkan] NVIDIA GPU: %%a ^(nvidia-smi not found - CUDA unavailable, using Vulkan^)
        set "GPU_NAME=%%a"
        set "GPU_VENDOR=nvidia"
        set "BACKEND=vulkan"
    )
)

REM --- AMD GPU Detection (ROCm / DirectML) ---
if not defined GPU_VENDOR (
    where rocm-smi >nul 2>&1
    if not errorlevel 1 (
        for /f "tokens=* " %%a in ('rocm-smi --showproductname 2^>nul ^| findstr "Card model:"') do (
            set "AMD_GPU=%%a"
        )
        if defined AMD_GPU (
            echo   [ROCm] AMD GPU detected
            set "GPU_VENDOR=amd"
            set "BACKEND=rocm"
        )
    )
)

REM --- AMD GPU via WMI fallback ---
if not defined GPU_VENDOR (
    for /f "delims=" %%a in ('powershell -NoProfile -Command "Get-CimInstance Win32_VideoController | Where-Object { $_.Name -match 'amd|radeon' } | ForEach-Object { $_.Name }" 2^>nul') do (
        echo   [DirectML] AMD GPU: %%a
        set "GPU_VENDOR=amd"
        set "BACKEND=directml"
    )
)

REM --- Intel GPU Detection ---
if not defined GPU_VENDOR (
    for /f "delims=" %%a in ('powershell -NoProfile -Command "Get-CimInstance Win32_VideoController | Where-Object { $_.Name -match 'intel iris|intel arc|intel uhd' } | ForEach-Object { $_.Name }" 2^>nul') do (
        echo   [DirectML] Intel GPU: %%a
        set "GPU_VENDOR=intel"
        set "BACKEND=directml"
    )
)

REM --- Vulkan Detection ---
if not defined GPU_VENDOR (
    where vulkaninfo >nul 2>&1
    if not errorlevel 1 (
        vulkaninfo --summary 2>nul | findstr /i "GPU" >nul
        if not errorlevel 1 (
            echo   [Vulkan] Vulkan-capable GPU detected
            set "GPU_VENDOR=vulkan"
            set "BACKEND=vulkan"
        )
    )
)

REM --- NPU Detection (AMD XDNA / Intel NPU, exclude false positives) ---
type nul > "%TEMP%\ghostlink_npu.txt" 2>nul
powershell -NoProfile -Command "& {Get-CimInstance -ClassName Win32_PnPEntity -ErrorAction SilentlyContinue | Where-Object { $_.Name -match '\b(NPU|Neural Processor|AI Accelerator|XDNA|Ryzen AI)\b' } | ForEach-Object { Write-Output $_.Name }}" > "%TEMP%\ghostlink_npu.txt" 2>nul
set "NPU_DETECTED="
for /f "delims=" %%a in ('type "%TEMP%\ghostlink_npu.txt" 2^>nul') do (
    if not "%%a"=="" (
        echo %%a | findstr /i "keyboard mouse hid usb input" >nul
        if errorlevel 1 (
            set "NPU_DETECTED=%%a"
        )
    )
)
del "%TEMP%\ghostlink_npu.txt" 2>nul
if defined NPU_DETECTED (
    echo   [NPU] !NPU_DETECTED!
)

REM --- GPU not detected ---
if not defined GPU_VENDOR (
    echo   No GPU detected - using CPU mode
    set "GPU_VENDOR=cpu"
    if not defined BACKEND set "BACKEND=cpu"
)

echo.

REM ==================== Manual Backend Override ====================
if not "%GHOSTLINK_LLAMA_BACKEND%"=="" (
    echo [INFO] Manual backend override: %GHOSTLINK_LLAMA_BACKEND%
    set "BACKEND=%GHOSTLINK_LLAMA_BACKEND%"
)

REM ==================== Resource Planning ====================
echo ====== Resource Planning ======
echo.

REM --- Threads ---
set "THREADS=%CPU_CORES%"
if %THREADS% GTR 1 set /a "THREADS=%CPU_CORES%-1"
if not "%GHOSTLINK_LLAMA_THREADS%"=="" set "THREADS=%GHOSTLINK_LLAMA_THREADS%"
echo   Threads: !THREADS! (of !CPU_CORES! cores)

REM --- GPU Layers (NGL) ---
set "LLAMA_NGL=0"
if not "%GHOSTLINK_LLAMA_NGL%"=="" (
    set "LLAMA_NGL=%GHOSTLINK_LLAMA_NGL%"
    if "!LLAMA_NGL!"=="-1" (
        echo   GPU Layers: all ^(user override^)
    ) else (
        echo   GPU Layers: !LLAMA_NGL! ^(user override^)
    )
) else (
    if defined GPU_VENDOR (
        if "%BACKEND%"=="cpu" (
            set "LLAMA_NGL=0"
            echo   GPU Layers: 0 ^(CPU mode^)
        ) else if defined VRAM_GB (
            if !VRAM_GB! GEQ 12 (
                set /a "LLAMA_NGL=40"
                echo   GPU Layers: 40 ^(VRAM-based, !VRAM_GB! GB^)
            ) else if !VRAM_GB! GEQ 8 (
                set /a "LLAMA_NGL=24"
                echo   GPU Layers: 24 ^(VRAM-based, !VRAM_GB! GB^)
            ) else if !VRAM_GB! GEQ 4 (
                set /a "LLAMA_NGL=12"
                echo   GPU Layers: 12 ^(VRAM-based, !VRAM_GB! GB^)
            ) else (
                set "LLAMA_NGL=8"
                echo   GPU Layers: 8 ^(VRAM-based, !VRAM_GB! GB^)
            )
        ) else (
            set "LLAMA_NGL=99"
            echo   GPU Layers: 99 ^(full offload^)
        )
    ) else (
        set "LLAMA_NGL=0"
        echo   GPU Layers: 0 ^(no GPU^)
    )
)

REM --- Backend flags for llama-server ---
set "BACKEND_FLAGS="
if "%BACKEND%"=="cuda" (
    set "BACKEND_FLAGS="
    echo   Backend: CUDA ^(NVIDIA^)
) else if "%BACKEND%"=="vulkan" (
    set "BACKEND_FLAGS="
    echo   Backend: Vulkan
) else if "%BACKEND%"=="rocm" (
    set "BACKEND_FLAGS="
    echo   Backend: ROCm/HIP ^(AMD^)
) else if "%BACKEND%"=="directml" (
    set "BACKEND_FLAGS="
    echo   Backend: DirectML ^(Windows^)
) else if "%BACKEND%"=="metal" (
    set "BACKEND_FLAGS="
    echo   Backend: Metal ^(Apple^)
) else (
    set "BACKEND_FLAGS=--no-cuda --no-vulkan --no-metal"
    echo   Backend: CPU
)

REM --- Memory lock ---
set "MLOCK_FLAG="
if %RAM_GB% GEQ 8 set "MLOCK_FLAG=--mlock"

echo.

REM ==================== Dependency Checks ====================
set "NEED_BUILD=0"
if not exist "%LLAMA_SERVER_EXPECTED%" if not exist "%LLAMA_SERVER_ALT_EXPECTED%" set "NEED_BUILD=1"

for %%c in (curl cargo node) do (
    where %%c >nul 2>&1
    if errorlevel 1 (
        echo [ERROR] %%c not found. Please install it.
        echo.
        pause
        exit /b 1
    ) else (
        echo [OK] %%c verified
    )
)

if "%NEED_BUILD%"=="1" (
    where cmake >nul 2>&1
    if errorlevel 1 (
        echo [ERROR] llama-server binary not found and cmake is required to build it.
        echo         Install cmake from https://cmake.org/download/ or add it to your PATH.
        echo.
        pause
        exit /b 1
    ) else (
        echo [OK] cmake verified
    )
)

echo.

REM ==================== Model Download ====================
if not exist "%MODEL_DIR%" mkdir "%MODEL_DIR%" 2>nul

if "%GHOSTLINK_SKIP_MODEL%"=="1" (
    echo [INFO] Skipping model check ^(GHOSTLINK_SKIP_MODEL=1^)
) else if not exist "!MODEL_FILE!" (
    echo [INFO] Default model not found. Starting without it ^(can download from Hugging Face tab^).
    echo [INFO] To pre-download: set GHOSTLINK_SKIP_MODEL=0 and ensure internet access.
) else (
    echo [OK] Default model found: !MODEL_FILE!
)

REM ==================== Find or Build llama-server ====================
set "ACTUAL_LLAMA_SERVER="
set "LLAMA_BUILT=0"

if "%GHOSTLINK_SKIP_BUILD%"=="1" (
    echo [INFO] Skipping build ^(GHOSTLINK_SKIP_BUILD=1^)
) else if not exist "%LLAMA_SERVER_EXPECTED%" if not exist "%LLAMA_SERVER_ALT_EXPECTED%" (
    echo [INFO] llama-server binary not found. Attempting to build from source...
    if not exist "%PROJECT_ROOT%third_party\llama.cpp" (
        echo [ERROR] Source directory not found: "%PROJECT_ROOT%third_party\llama.cpp"
        echo         Run: git submodule update --init --recursive
        echo.
        pause
        exit /b 1
    )
    pushd "%PROJECT_ROOT%third_party\llama.cpp"
    if not exist "build" md build
    pushd build

    REM Select cmake GPU flags based on detected backend
    set "CMAKE_GPU_FLAGS="
    if "%BACKEND%"=="cuda" (
        set "CMAKE_GPU_FLAGS=-DLLAMA_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES=all"
        echo [INFO] Building with CUDA support
    ) else if "%BACKEND%"=="vulkan" (
        set "CMAKE_GPU_FLAGS=-DLLAMA_VULKAN=ON"
        echo [INFO] Building with Vulkan support
    ) else if "%BACKEND%"=="rocm" (
        set "CMAKE_GPU_FLAGS=-DLLAMA_HIPBLAS=ON -DAMDGPU_TARGETS=all"
        echo [INFO] Building with ROCm/HIP support
    ) else if "%BACKEND%"=="directml" (
        set "CMAKE_GPU_FLAGS=-DLLAMA_DIRECTML=ON"
        echo [INFO] Building with DirectML support
    ) else (
        set "CMAKE_GPU_FLAGS="
        echo [INFO] Building CPU-only
    )

    echo [INFO] Running cmake configuration...
    cmake .. -DCMAKE_BUILD_TYPE=Release %CMAKE_GPU_FLAGS%
    if not errorlevel 1 (
        echo [INFO] Building llama-server ^(this may take a few minutes^)...
        cmake --build . --config Release --target llama-server
        if not errorlevel 1 (
            echo [OK] llama-server built successfully
            set "LLAMA_BUILT=1"
        ) else (
            echo [ERROR] Build failed. Try building manually in third_party\llama.cpp\build
        )
    ) else (
        echo [ERROR] CMake configuration failed.
    )
    popd
    popd
)

REM Post-build: verify binary exists at expected location
echo [INFO] Verifying llama-server binary...
if exist "%LLAMA_SERVER_EXPECTED%" (
    set "ACTUAL_LLAMA_SERVER=%LLAMA_SERVER_EXPECTED%"
    echo [OK] Found llama-server at %ACTUAL_LLAMA_SERVER%
) else if exist "%LLAMA_SERVER_ALT_EXPECTED%" (
    set "ACTUAL_LLAMA_SERVER=%LLAMA_SERVER_ALT_EXPECTED%"
    echo [OK] Found llama-server at %ACTUAL_LLAMA_SERVER%
) else (
    echo [WARN] llama-server binary not found at expected locations
    echo        Expected: %LLAMA_SERVER_EXPECTED%
    echo        Alt:      %LLAMA_SERVER_ALT_EXPECTED%
    set "ACTUAL_LLAMA_SERVER="
)

echo.

REM ==================== Start llama-server ====================
set "GHOSTLINK_NATIVE_ENGINE=simulated"

REM FIX: Export variables for child processes and use proper quoting
set "LLAMA_CMD=%ACTUAL_LLAMA_SERVER%"
set "LLAMA_MODEL=%MODEL_FILE%"
set "LLAMA_HOST=127.0.0.1"
set "LLAMA_PORT_VAL=%LLAMA_PORT%"
set "LLAMA_NGL_VAL=!LLAMA_NGL!"
set "LLAMA_THREADS_VAL=!THREADS!"
if defined MLOCK_FLAG set "LLAMA_MLOCK=%MLOCK_FLAG%" else set "LLAMA_MLOCK="
set "LLAMA_BACKEND_FLAGS=%BACKEND_FLAGS%"

REM Start llama-server in a new window with properly exported variables
start "llama-server" cmd /c ^
    "set LLAMA_CMD=%LLAMA_CMD^&^&set LLAMA_MODEL=%LLAMA_MODEL^&^&set LLAMA_HOST=%LLAMA_HOST^&^&set LLAMA_PORT_VAL=%LLAMA_PORT_VAL^&^&set LLAMA_NGL_VAL=!LLAMA_NGL_VAL!^&^&set LLAMA_THREADS_VAL=!LLAMA_THREADS_VAL!^&^&if defined LLAMA_MLOCK set LLAMA_MLOCK=%LLAMA_MLOCK% ^&^&set LLAMA_BACKEND_FLAGS=%LLAMA_BACKEND_FLAGS%^&^&cmd /k \"%LLAMA_CMD% -m %LLAMA_MODEL% --host %LLAMA_HOST% --port %LLAMA_PORT_VAL% -ngl %LLAMA_NGL_VAL% -t %LLAMA_THREADS_VAL% -np 1 %LLAMA_MLOCK% %LLAMA_BACKEND_FLAGS%\""

echo [INFO] Starting llama-server on http://127.0.0.1:%LLAMA_PORT%
echo [INFO] Flags: -ngl !LLAMA_NGL! -t !THREADS! %MLOCK_FLAG% %BACKEND_FLAGS%

REM FIX: Wait for llama-server with better timeout and error handling
echo [INFO] Waiting for llama-server...
set /a "LLAMA_WAIT=0"
:WAIT_LLAMA
curl -sf http://127.0.0.1:%LLAMA_PORT%/health >nul 2>&1
if not errorlevel 1 goto LLAMA_READY
set /a "LLAMA_WAIT+=1"
if %LLAMA_WAIT% GEQ 60 (
    echo [WARN] llama-server not healthy after ~60s. Continuing in simulated mode.
    echo [WARN] Check the llama-server window for errors.
    goto LLAMA_DONE
)
ping -n 2 127.0.0.1 >nul
goto WAIT_LLAMA
:LLAMA_READY
echo [OK] llama-server is healthy
set "GHOSTLINK_NATIVE_ENGINE=llama_server"
:LLAMA_DONE

set "GHOSTLINK_INFERENCE_BACKEND=native"
set "GHOSTLINK_LLAMA_SERVER_URL=http://127.0.0.1:!LLAMA_PORT!/completion"

REM ==================== Start Backend ====================
echo [INFO] Starting Ghostlink Backend API...

set "GHOSTLINK_BINARY="
if exist "%PROJECT_ROOT%target\release\ghost-link.exe" set "GHOSTLINK_BINARY=%PROJECT_ROOT%target\release\ghost-link.exe"
if exist "%PROJECT_ROOT%target\debug\ghost-link.exe" if "!GHOSTLINK_BINARY!"=="" set "GHOSTLINK_BINARY=%PROJECT_ROOT%target\debug\ghost-link.exe"

if defined GHOSTLINK_BINARY (
    echo [INFO] Starting backend binary: !GHOSTLINK_BINARY!
    start "Ghostlink Backend API" cmd /c "!GHOSTLINK_BINARY! serve %BACKEND_HOST% %BACKEND_PORT%"
) else (
    echo [INFO] Building backend with Cargo...
    start "Ghostlink Backend API" cmd /c "cd /d \"%PROJECT_ROOT%\" ^&^& cargo run -p ghost-link -- serve %BACKEND_HOST% %BACKEND_PORT%"
)

REM Give backend a moment to fully initialize routes
echo [INFO] Waiting for backend to fully initialize...
ping -n 3 127.0.0.1 >nul

REM Wait for backend (first launch may compile with Cargo, allow up to ~10 min)
echo [INFO] Waiting for API...
set /a "API_WAIT=0"
:WAIT_API
curl -sf http://%BACKEND_HOST%:%BACKEND_PORT%/health >nul 2>&1
if not errorlevel 1 goto API_HEALTHY
set /a "API_WAIT+=1"
if %API_WAIT% GEQ 600 (
    echo [ERROR] Backend API did not respond within ~10 minutes.
    echo         Check the "Ghostlink Backend API" window for build or startup errors.
    pause
    exit /b 1
)
ping -n 2 127.0.0.1 >nul
goto WAIT_API
:API_HEALTHY
echo [OK] Backend API is healthy

REM Wait for API endpoint to be fully ready - give backend extra time to register all routes
echo [INFO] Waiting for API endpoint...
ping -n 4 127.0.0.1 >nul
set /a "API_V2_WAIT=0"
:WAIT_API_V2
curl -sf http://%BACKEND_HOST%:%BACKEND_PORT%/api/health >nul 2>&1
if not errorlevel 1 goto API_V2_READY
set /a "API_V2_WAIT+=1"
if %API_V2_WAIT% GEQ 20 (
    echo [WARN] /api/health not responding after ~60s. Continuing anyway.
    goto API_V2_DONE
)
ping -n 4 127.0.0.1 >nul
goto WAIT_API_V2
:API_V2_READY
echo [OK] API endpoint is ready
:API_V2_DONE

REM ==================== Start GUI ====================
echo [INFO] Starting GUI on http://127.0.0.1:%GUI_PORT%
cd "%PROJECT_ROOT%ghostlink_gui_modern"

if not exist "node_modules" (
    echo [INFO] Installing npm dependencies...
    call npm install --legacy-peer-deps
    if errorlevel 1 (
        echo [ERROR] npm install failed
        pause
        exit /b 1
    )
)

REM Set API base to match backend host — inherited by child cmd windows
set "VITE_GHOSTLINK_API_BASE=http://%BACKEND_HOST%:%BACKEND_PORT%"

REM Start frontend dev server (with Vite proxy to backend)
echo [INFO] Starting GUI dev server...
start "Ghostlink Studio GUI" cmd /c "npm run dev -- --host 127.0.0.1 --port %GUI_PORT%"

cd "%PROJECT_ROOT%"

REM Wait for GUI
echo [INFO] Waiting for frontend...
set /a "GUI_WAIT=0"
:WAIT_GUI
curl -sf http://127.0.0.1:%GUI_PORT% >nul 2>&1
if not errorlevel 1 goto GUI_READY
set /a "GUI_WAIT+=1"
if %GUI_WAIT% GEQ 120 (
    echo [WARN] Frontend not responding after ~2 minutes. Continuing anyway.
    echo [WARN] Check the "Ghostlink Studio GUI" window for errors.
    goto GUI_DONE
)
ping -n 2 127.0.0.1 >nul
goto WAIT_GUI
:GUI_READY
echo [OK] GUI is ready
:GUI_DONE

echo.
echo ========================================================================
echo   Ghostlink Studio is Ready!
echo ========================================================================
echo.
echo   Web UI:        http://127.0.0.1:%GUI_PORT%
echo   Backend API:   http://%BACKEND_HOST%:%BACKEND_PORT%
echo   llama-server:  http://127.0.0.1:%LLAMA_PORT%
echo.
echo   Hardware:      !GPU_NAME_1! (!BACKEND! backend, !LLAMA_NGL! GPU layers)
echo   System:        !THREADS! threads, !RAM_GB! GB RAM, !GPU_COUNT! GPU(s)
if defined NPU_DETECTED echo   NPU:           !NPU_DETECTED!
echo.
echo   To stop: Close the terminal windows or press Ctrl+C
echo.
echo   Opening browser...
echo ========================================================================

start "" "http://127.0.0.1:%GUI_PORT%"

pause
