@echo off
REM Ghostlink Studio - Cinematic Launch Script (Windows)
REM Native llama.cpp + Ghostlink API + React GUI

setlocal enabledelayedexpansion

REM Ensure cmake is on PATH (common install locations)
where cmake >nul 2>&1
if errorlevel 1 (
    if exist "C:\Program Files\CMake\bin\cmake.exe" set "PATH=C:\Program Files\CMake\bin;%PATH%"
    if exist "C:\Program Files (x86)\CMake\bin\cmake.exe" set "PATH=C:\Program Files (x86)\CMake\bin;%PATH%"
)

REM Configuration
set "BACKEND_HOST=127.0.0.1"
set "BACKEND_PORT=8003"
set "GUI_PORT=5173"
set "LLAMA_PORT=8080"
set "LLAMA_NGL=%GHOSTLINK_LLAMA_NGL%"
if "%LLAMA_NGL%"=="" set "LLAMA_NGL=-1"

REM Auto-detect GPU: check NVIDIA, AMD (DirectML/Vulkan), Intel, NPU, then fall back to CPU
set "GPU_VENDOR="
set "NPU_DETECTED=0"
set "LLAMA_GPU_BACKEND="
set "CMAKE_GPU_FLAGS="
where nvidia-smi >nul 2>&1
if not errorlevel 1 (
    for /f "tokens=*" %%a in ('nvidia-smi --query-gpu=name --format=csv,noheader 2^>nul') do set "GPU_NAME=%%a"
    if defined GPU_NAME (
        echo %GREEN%  ⚡ NVIDIA GPU detected: %GPU_NAME%%NC%
        set "GPU_VENDOR=nvidia"
        set "LLAMA_GPU_BACKEND=CUDA"
    )
)
if not defined GPU_VENDOR (
    where rocm-smi >nul 2>&1
    if not errorlevel 1 (
        for /f "tokens=*" %%a in ('rocm-smi --showproductname 2^>nul ^| findstr "Card model:"') do set "GPU_NAME=%%a"
        if defined GPU_NAME (
            echo %GREEN%  ⚡ AMD GPU detected: %GPU_NAME%%NC%
            set "GPU_VENDOR=amd"
            set "LLAMA_GPU_BACKEND=HIP"
        ) else (
            echo %GREEN%  ⚡ AMD ROCm detected (rocm-smi)%NC%
            set "GPU_VENDOR=amd"
            set "LLAMA_GPU_BACKEND=HIP"
        )
    )
)
REM For AMD GPUs on Windows, prefer Vulkan over HIP (ROCm doesn't support most AMD iGPUs)
if not defined GPU_VENDOR (
    for /f "skip=1 tokens=2 delims=," %%a in ('wmic path Win32_VideoController get Name /format:csv 2^>nul') do (
        echo %%a | findstr /i "amd radeon advanced.micro" >nul
        if not errorlevel 1 (
            set "GPU_NAME=%%a"
            echo %GREEN%  ⚡ AMD GPU detected: %%a%NC%
            set "GPU_VENDOR=amd"
            REM AMD iGPUs (Radeon 8xxM series etc.) use DirectML/Vulkan on Windows, not HIP/ROCm
            echo %%a | findstr /i "radeon.*m" >nul
            if not errorlevel 1 (
                set "LLAMA_GPU_BACKEND=Vulkan"
                echo %DIM%  ⚡ AMD integrated GPU — using Vulkan backend%NC%
            ) else (
                set "LLAMA_GPU_BACKEND=Vulkan"
            )
        )
    )
)
if not defined GPU_VENDOR (
    for /f "skip=1 tokens=2 delims=," %%a in ('wmic path Win32_VideoController get Name /format:csv 2^>nul') do (
        echo %%a | findstr /i "intel iris arc" >nul
        if not errorlevel 1 (
            set "GPU_NAME=%%a"
            echo %GREEN%  ⚡ Intel GPU detected: %%a%NC%
            set "GPU_VENDOR=intel"
            set "LLAMA_GPU_BACKEND=Vulkan"
        )
    )
)
REM Add AMD NPU detection (Ryzen AI / XDNA)
powershell -NoProfile -Command "
    $npus = Get-CimInstance -Namespace 'root\cimv2' -ClassName Win32_PnPEntity 2>$null | Where-Object { $_.PNPClass -eq 'System' -and $_.Name -match '(NPU|Neural|AI Accelerator|XDNA|Ryzen AI)' }
    if (-not $npus) { $npus = Get-CimInstance -Namespace 'root\cimv2' -ClassName Win32_PnPEntity 2>$null | Where-Object { $_.Name -match '(NPU|Neural|AI Accelerator|XDNA|Ryzen AI)' } }
    if ($npus) { Write-Output 'NPU_FOUND' }
" >nul 2>&1 && (
    echo %GREEN%  ⚡ AMD Ryzen AI NPU detected%NC%
    set "NPU_DETECTED=1"
)
if not defined GPU_VENDOR (
    if "%LLAMA_NGL%"=="-1" (
        echo %YELLOW%  ⚡ No GPU detected - using CPU mode (ngl=0)%NC%
        echo %YELLOW%  ⚡ Set GHOSTLINK_LLAMA_NGL or install a GPU driver for acceleration%NC%
        set "LLAMA_NGL=0"
    )
) else (
    if "%LLAMA_GPU_BACKEND%"=="Vulkan" echo %DIM%  ⚡ Using Vulkan backend for GPU acceleration%NC%
    if "%LLAMA_GPU_BACKEND%"=="CUDA" echo %DIM%  ⚡ NVIDIA GPU will use CUDA backend via llama.cpp%NC%
    if "%LLAMA_GPU_BACKEND%"=="HIP" echo %DIM%  ⚡ AMD GPU will use ROCm/HIP backend via llama.cpp%NC%
    if "%NPU_DETECTED%"=="1" echo %DIM%  ⚡ NPU acceleration available for supported models%NC%
)
set "MODEL_DIR=.\models"
set "MODEL_FILE=%MODEL_DIR%\stories15M-q4_0.gguf"
set "MODEL_URL=https://huggingface.co/ggml-org/models/resolve/main/tinyllamas/stories15M-q4_0.gguf"

REM Check if ports are already in use
echo Checking port availability...
for %%P in (%BACKEND_PORT% %GUI_PORT% %LLAMA_PORT%) do (
    netstat -ano | findstr "%%P" | findstr "LISTENING" >nul 2>&1
    if not errorlevel 1 (
        echo %RED%[ERROR] Port %%P is already in use. Please stop the existing service or use a different port.%NC%
        pause
        exit /b 1
    )
)
echo %GREEN%[OK] All ports are available%NC%
echo.

REM Read model_path from settings.json if available (PowerShell JSON parsing)
if exist "settings.json" (
    for /f "usebackq tokens=*" %%a in (`powershell -NoProfile -Command "try { $s = Get-Content 'settings.json' -Raw | ConvertFrom-Json; if ($s.model_path) { Write-Output $s.model_path } } catch {}"`) do (
        set "MODEL_FILE=%%a"
    )
)
set "LLAMA_CPP_DIR=third_party\llama.cpp"
set "LLAMA_SERVER=%LLAMA_CPP_DIR%\build\bin\Release\llama-server.exe"
set "LLAMA_SERVER_ALT=%LLAMA_CPP_DIR%\build\bin\llama-server.exe"

REM Colors (ANSI)
for /f %%a in ('echo prompt $e ^| cmd') do set "ESC=%%a"
set "RED=%ESC%[0;31m"
set "GREEN=%ESC%[0;32m"
set "YELLOW=%ESC%[1;33m"
set "BLUE=%ESC%[0;34m"
set "CYAN=%ESC%[0;36m"
set "MAGENTA=%ESC%[0;35m"
set "WHITE=%ESC%[1;37m"
set "GRAY=%ESC%[0;37m"
set "BOLD=%ESC%[1m"
set "DIM=%ESC%[2m"
set "NC=%ESC%[0m"
set "CLEAR_LINE=%ESC%[2K\r"

REM Enable ANSI processing
reg add HKCU\Console /v VirtualTerminalLevel /t REG_DWORD /d 1 /f >nul 2>&1

cls

REM ========== CINEMATIC BANNER ==========
echo %CYAN%
echo ╔════════════════════════════════════════════════════════════════════════════════╗
echo ║                                                                                ║
echo ║          %WHITE%███████╗██╗  ██╗ ██████╗ ███████╗████████╗██╗     ██╗███╗   ██╗██╗  ██╗%CYAN%          ║
echo ║          %WHITE%██╔════╝██║  ██║██╔═══██╗██╔════╝╚══██╔══╝██║     ██║████╗  ██║██║ ██╔╝%CYAN%          ║
echo ║          %WHITE%███████╗███████║██║   ██║███████╗   ██║   ██║     ██║██╔██╗ ██║█████╔╝ %CYAN%           ║
echo ║          %WHITE%╚════██║██╔══██║██║   ██║╚════██║   ██║   ██║     ██║██║╚██╗██║██╔═██╗ %CYAN%           ║
echo ║          %WHITE%███████║██║  ██║╚██████╔╝███████║   ██║   ███████╗██║██║ ╚████║██║  ██╗%CYAN%          ║
echo ║          %WHITE%╚══════╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝   ╚═╝   ╚══════╝╚═╝╚═╝  ╚═══╝╚═╝  ╚═╝%CYAN%          ║
echo ║                                                                                ║
echo ║                 %WHITE%Distributed LLM Inference Fabric%CYAN%                         ║
echo ║                 %GRAY%Enterprise AI Model Management%CYAN%                            ║
echo ║                                                                                ║
echo ╚═══════════════════════════════════════════════════════════════════════════════╝
echo %NC%
echo.

REM ========== SYSTEM INFO ==========
echo %BLUE%System Information:%NC%
ver | findstr /r /c:"Version" >nul
for /f "tokens=2 delims=[]" %%a in ('ver') do echo   OS: %%a
node -v 2>nul && for /f "tokens=*" %%a in ('node -v') do echo   Node.js: %%a || echo   Node.js: not installed
npm -v 2>nul && for /f "tokens=*" %%a in ('npm -v') do echo   npm: %%a || echo   npm: not installed
echo.

REM ========== COMPONENT CHECK ==========
echo %BLUE%Checking Components:%NC%
echo.

set "BACKEND_FOUND=0"
set "GUI_FOUND=0"
set "NATIVE_FOUND=0"
set "LLAMA_BUILT=0"

REM Check backend binary
if exist "target\release\ghost-link.exe" (
    echo   %BLUE%│%NC%  %WHITE%Ghostlink API Binary%NC%        %GREEN%✓ Found%NC%  %DIM%(target\release\ghost-link.exe)%NC%
    set "BACKEND_FOUND=1"
) else if exist "target\debug\ghost-link.exe" (
    echo   %BLUE%│%NC%  %WHITE%Ghostlink API Binary%NC%        %YELLOW%⚠ Debug build%NC%  %DIM%(target\debug\ghost-link.exe)%NC%
    set "BACKEND_FOUND=1"
) else (
    echo   %BLUE%│%NC%  %WHITE%Ghostlink API Binary%NC%        %RED%✗ Not built%NC%  %DIM%(run: cargo build --release -p ghost-link)%NC%
)

REM Check GUI
if exist "ghostlink_gui_modern\package.json" (
    echo   %BLUE%│%NC%  %WHITE%React Frontend%NC%              %GREEN%✓ Found%NC%  %DIM%(ghostlink_gui_modern/)%NC%
    set "GUI_FOUND=1"
) else (
    echo   %BLUE%│%NC%  %WHITE%React Frontend%NC%              %RED%✗ Missing%NC%
)

REM Check native stack launcher
if exist "scripts\run_native_llama_server_stack.sh" (
    echo   %BLUE%│%NC%  %WHITE%Native Stack Launcher%NC%       %GREEN%✓ Found%NC%  %DIM%(scripts\run_native_llama_server_stack.sh)%NC%
    set "NATIVE_FOUND=1"
) else (
    echo   %BLUE%│%NC%  %WHITE%Native Stack Launcher%NC%       %RED%✗ Missing%NC%
)

REM Check llama.cpp build
if exist "%LLAMA_SERVER%" (
    echo   %BLUE%│%NC%  %WHITE%llama.cpp (llama-server)%NC%     %GREEN%✓ Built%NC%   %DIM%(%LLAMA_SERVER%)%NC%
    set "LLAMA_BUILT=1"
) else if exist "%LLAMA_SERVER_ALT%" (
    echo   %BLUE%│%NC%  %WHITE%llama.cpp (llama-server)%NC%     %GREEN%✓ Built%NC%   %DIM%(%LLAMA_SERVER_ALT%)%NC%
    set "LLAMA_BUILT=1"
) else (
    echo   %BLUE%│%NC%  %WHITE%llama.cpp (llama-server)%NC%     %YELLOW%⚠ Not built%NC% %DIM%(will build on first launch)%NC%
)

REM Model directory
if not exist "%MODEL_DIR%" mkdir "%MODEL_DIR%" 2>nul
echo   %BLUE%│%NC%  %WHITE%Model Directory%NC%             %GREEN%✓ Ready%NC%   %DIM%(%MODEL_DIR%)%NC%
echo   %BLUE%└────────────────────────────────────────────────────────────────────────────────────%NC%
echo.

REM ========== PRE-REQUISITES ==========
echo %BLUE%Verifying Prerequisites:%NC%
cargo --version >nul 2>&1
if errorlevel 1 (
    echo   %RED%✗%NC% Rust/Cargo not found. Install from https://rustup.rs/
    pause
    exit /b 1
)
echo   %GREEN%✓%NC% Rust/Cargo verified

node --version >nul 2>&1
if errorlevel 1 (
    echo   %RED%✗%NC% Node.js not found. Install from https://nodejs.org/
    pause
    exit /b 1
)
echo   %GREEN%✓%NC% Node.js verified

cmake --version >nul 2>&1
if errorlevel 1 (
    echo   %RED%✗%NC% CMake not found. Required for building llama.cpp.
    echo       Install: winget install Kitware.CMake
    echo       Or download from https://cmake.org/download/
    pause
    exit /b 1
)
echo   %GREEN%✓%NC% CMake verified

REM ========== CINEMATIC INTRO ==========
echo %DIM%  Initializing distributed LLM inference fabric...%NC%
ping -n 2 127.0.0.1 >nul
echo %DIM%  Loading neural pathways...%NC%
ping -n 2 127.0.0.1 >nul
echo %DIM%  Calibrating tensor cores...%NC%
ping -n 2 127.0.0.1 >nul
echo.

REM ========== START SERVICES ==========
echo %BLUE%╔════════════════════════════════════════════════════════════════════════════════╗%NC%
echo %BLUE%║%NC%                          %BOLD%STARTING SERVICES%NC%                                  %BLUE%║%NC%
echo %BLUE%╠════════════════════════════════════════════════════════════════════════════════╣%NC%
echo.

REM ---- 1. BUILD LLAMA.CPP IF NEEDED ----
if "%LLAMA_BUILT%"=="0" (
    echo   %WHITE%▶%NC% %BOLD%Building llama.cpp (llama-server)%NC% %DIM%(one-time, 2-5 minutes)%NC%
    echo   %DIM%  Cloning llama.cpp...%NC%
    if not exist "%LLAMA_CPP_DIR%" (
        mkdir "%LLAMA_CPP_DIR:~0,-9%" 2>nul
        git clone https://github.com/ggml-org/llama.cpp.git "%LLAMA_CPP_DIR%" >nul 2>&1
    )
    
     echo   %DIM%  Configuring CMake (Release)...%NC%
     REM Select GPU backend: NVIDIA CUDA, AMD HIP/Vulkan, Intel Vulkan, or CPU-only
     set "CMAKE_GPU_FLAGS="
     if "%GPU_VENDOR%"=="nvidia" (
         set "CMAKE_GPU_FLAGS=-DLLAMA_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES=all"
         echo   %DIM%  NVIDIA GPU detected — building with CUDA support%NC%
     )
     if "%GPU_VENDOR%"=="amd" (
         if "%LLAMA_GPU_BACKEND%"=="HIP" (
             set "CMAKE_GPU_FLAGS=-DLLAMA_HIPBLAS=ON -DAMDGPU_TARGETS=all"
             echo   %DIM%  AMD GPU detected — building with HIP/ROCm support%NC%
         ) else (
             REM Use Vulkan for AMD iGPUs (ROCm doesn't support RDNA3.5+ iGPUs on Windows)
             set "CMAKE_GPU_FLAGS=-DLLAMA_VULKAN=ON -DLLAMA_VULKAN_RUN_TESTS=OFF"
             echo   %DIM%  AMD GPU detected — building with Vulkan support (DirectML)%NC%
         )
     )
     if "%GPU_VENDOR%"=="intel" (
         set "CMAKE_GPU_FLAGS=-DLLAMA_VULKAN=ON -DLLAMA_VULKAN_RUN_TESTS=OFF"
         echo   %DIM%  Intel GPU detected — building with Vulkan support%NC%
     )
     if "%GPU_VENDOR%"=="" (
         echo   %DIM%  No GPU detected — building CPU-only llama.cpp%NC%
     )
     cmake -S "%LLAMA_CPP_DIR%" -B "%LLAMA_CPP_DIR%\build" -DCMAKE_BUILD_TYPE=Release %CMAKE_GPU_FLAGS%
    if errorlevel 1 (
        echo   %RED%✗%NC% CMake configuration failed
        echo   %YELLOW%  Retrying with CPU-only flags...%NC%
        cmake -S "%LLAMA_CPP_DIR%" -B "%LLAMA_CPP_DIR%\build" -DCMAKE_BUILD_TYPE=Release >nul 2>&1
        if errorlevel 1 (
            echo   %RED%✗%NC% CMake configuration failed
            pause
            exit /b 1
        )
    )
    
    echo   %DIM%  Compiling llama-server...%NC%
    cmake --build "%LLAMA_CPP_DIR%\build" --config Release --target llama-server -j >nul 2>&1
    if errorlevel 1 (
        echo   %RED%✗%NC% Build failed. Check CMake output.
        pause
        exit /b 1
    )
    
    if exist "%LLAMA_SERVER%" (
        echo   %GREEN%✓%NC% llama-server built successfully
    ) else if exist "%LLAMA_SERVER_ALT%" (
        echo   %GREEN%✓%NC% llama-server built successfully
    ) else (
        echo   %RED%✗%NC% Build succeeded but binary not found
        pause
        exit /b 1
    )
    echo.
)

REM ---- 2. DOWNLOAD MODEL IF NEEDED ----
if not exist "!MODEL_FILE!" (
    if "!MODEL_FILE!"=="%MODEL_DIR%\stories15M-q4_0.gguf" (
        echo   %WHITE%▶%NC% %BOLD%Downloading Model%NC% %DIM%(stories15M-q4_0.gguf ~15MB)%NC%
        curl -L --fail -o "!MODEL_FILE!" "%MODEL_URL%" >nul 2>&1
        if errorlevel 1 (
            echo   %RED%✗%NC% Model download failed
            pause
            exit /b 1
        )
        echo   %GREEN%✓%NC% Model downloaded
    ) else (
        echo   %YELLOW%⚠%NC% Model not found: !MODEL_FILE!
        echo   %YELLOW%  Download from the Ghostlink Studio UI (Models tab → Hugging Face)%NC%
        echo   %YELLOW%  Using default model for now...%NC%
        set "MODEL_FILE=%MODEL_DIR%\stories15M-q4_0.gguf"
        if not exist "!MODEL_FILE!" (
            curl -L --fail -o "!MODEL_FILE!" "%MODEL_URL%" >nul 2>&1
        )
    )
) else (
    echo   %WHITE%▶%NC% %BOLD%Model%NC% %GREEN%✓ Already present%NC% %DIM%(!MODEL_FILE!)%NC%
)
echo.

REM ---- 3. START LLAMA-SERVER ----
echo   %WHITE%▶%NC% %BOLD%Native Inference Stack%NC% %DIM%(llama-server + Ghostlink API)%NC%
echo   %DIM%  Starting llama-server on port %LLAMA_PORT% (ngl=%LLAMA_NGL%)...%NC%

REM Determine which binary to use
set "ACTUAL_LLAMA_SERVER="
if exist "%LLAMA_SERVER%" set "ACTUAL_LLAMA_SERVER=%LLAMA_SERVER%"
if exist "%LLAMA_SERVER_ALT%" if "%ACTUAL_LLAMA_SERVER%"=="" set "ACTUAL_LLAMA_SERVER=%LLAMA_SERVER_ALT%"

start "llama-server" cmd /k ""!ACTUAL_LLAMA_SERVER!" -m "!MODEL_FILE!" --host 127.0.0.1 --port %LLAMA_PORT% -ngl !LLAMA_NGL!"

echo   %DIM%  Waiting for llama-server health check...%NC%
:WAIT_LLAMA
curl -sf http://127.0.0.1:%LLAMA_PORT%/health >nul 2>&1
if errorlevel 1 (
    ping -n 2 127.0.0.1 >nul
    goto WAIT_LLAMA
)
echo   %GREEN%✓%NC% llama-server is healthy
echo.

REM ---- 4. START GHOSTLINK API ----
echo   %WHITE%▶%NC% %BOLD%Ghostlink API Server%NC% %DIM%(port %BACKEND_PORT%)%NC%
echo   %DIM%  Starting...%NC%

set "GHOSTLINK_INFERENCE_BACKEND=native"
set "GHOSTLINK_NATIVE_ENGINE=llama_server"
set "GHOSTLINK_LLAMA_SERVER_URL=http://127.0.0.1:%LLAMA_PORT%/completion"

REM Use pre-built binary if available, otherwise fall back to cargo run
set "GHOSTLINK_BINARY="
if exist "target\release\ghost-link.exe" set "GHOSTLINK_BINARY=target\release\ghost-link.exe"
if exist "target\debug\ghost-link.exe" if "%GHOSTLINK_BINARY%"=="" set "GHOSTLINK_BINARY=target\debug\ghost-link.exe"

if not "%GHOSTLINK_BINARY%"=="" (
    echo %DIM%  Using pre-built binary: %GHOSTLINK_BINARY%%NC%
    start "Ghostlink API" cmd /k ""%GHOSTLINK_BINARY%" serve %BACKEND_HOST% %BACKEND_PORT%"
) else (
    echo %YELLOW%  No pre-built binary found — building with cargo...%NC%
    start "Ghostlink API" cmd /k "cargo run -p ghost-link -- serve %BACKEND_HOST% %BACKEND_PORT%"
)

echo   %DIM%  Waiting for Ghostlink API health check...%NC%
:WAIT_API
curl -sf http://%BACKEND_HOST%:%BACKEND_PORT%/health >nul 2>&1
if errorlevel 1 (
    ping -n 2 127.0.0.1 >nul
    goto WAIT_API
)
echo   %GREEN%✓%NC% Ghostlink API is healthy
echo.

REM ---- 5. START REACT FRONTEND ----
echo   %WHITE%▶%NC% %BOLD%React Frontend (Vite)%NC% %DIM%(port %GUI_PORT%)%NC%
cd ghostlink_gui_modern

if not exist "node_modules" (
    echo   %DIM%  Installing npm dependencies...%NC%
    npm install --legacy-peer-deps >nul 2>&1
    if errorlevel 1 (
        echo   %RED%✗%NC% npm install failed
        pause
        exit /b 1
    )
    echo   %GREEN%✓%NC% Dependencies installed
) else (
    echo   %GREEN%✓%NC% Dependencies cached
)

start "Ghostlink GUI" cmd /k "npm run dev -- --host 127.0.0.1 --port %GUI_PORT%"
cd ..

echo   %DIM%  Waiting for Vite dev server...%NC%
:WAIT_GUI
curl -sf http://127.0.0.1:%GUI_PORT% >nul 2>&1
if errorlevel 1 (
    ping -n 2 127.0.0.1 >nul
    goto WAIT_GUI
)
echo   %GREEN%✓%NC% React Frontend is healthy
echo.

echo %BLUE%╠════════════════════════════════════════════════════════════════════════════════╣%NC%
echo.

REM ========== SUCCESS SCREEN ==========
echo %GREEN%╔════════════════════════════════════════════════════════════════════════════════╗%NC%
echo %GREEN%║%NC%                                                                                 %GREEN%║%NC%
echo %GREEN%║%NC%   %BOLD%%WHITE%███████╗██╗  ██╗██████╗ ███████╗████████╗██╗   ██╗███████╗%NC%                        %GREEN%║%NC%
echo %GREEN%║%NC%   %BOLD%%WHITE%██╔════╝██║  ██║██╔══██╗██╔════╝╚══██╔══╝██║   ██║██╔════╝%NC%                        %GREEN%║%NC%
echo %GREEN%║%NC%   %BOLD%%WHITE%█████╗  ███████║██████╔╝█████╗     ██║   ██║   ██║███████╗%NC%                        %GREEN%║%NC%
echo %GREEN%║%NC%   %BOLD%%WHITE%██╔══╝  ██╔══██║██╔══██╗██╔══╝     ██║   ██║   ██║╚════██║%NC%                        %GREEN%║%NC%
echo %GREEN%║%NC%   %BOLD%%WHITE%██║     ██║  ██║██║  ██║███████╗   ██║   ╚██████╔╝███████║%NC%                        %GREEN%║%NC%
echo %GREEN%║%NC%   %BOLD%%WHITE%╚═╝     ╚═╝  ╚═╝╚═╝  ╚═╝╚══════╝   ╚═╝    ╚═════╝ ╚══════╝%NC%                        %GREEN%║%NC%
echo %GREEN%║%NC%                                                                                 %GREEN%║%NC%
echo %GREEN%║%NC%   %CYAN%Ghostlink Studio is now running!%NC%                                              %GREEN%║%NC%
echo %GREEN%║%NC%                                                                                 %GREEN%║%NC%
echo %GREEN%╚══════════════════════════════════════════════════════════════════════════════════╝%NC%
echo.

echo %BLUE%╔════════════════════════════════════════════════════════════════════════════════╗%NC%
echo %BLUE%║%NC%  %WHITE%▶%NC% %BOLD%Web Interface%NC%      → %CYAN%http://127.0.0.1:%GUI_PORT%%NC%                            %BLUE%║%NC%
echo %BLUE%║%NC%  %WHITE%▶%NC% %BOLD%API Server%NC%         → %CYAN%http://%BACKEND_HOST%:%BACKEND_PORT%%NC%                            %BLUE%║%NC%
echo %BLUE%║%NC%  %WHITE%▶%NC% %BOLD%Native Inference%NC%   → %CYAN%http://127.0.0.1:%LLAMA_PORT%%NC% %DIM%(llama-server)%NC%                     %BLUE%║%NC%
echo %BLUE%╚════════════════════════════════════════════════════════════════════════════════╝%NC%
echo.

echo %YELLOW%╔════════════════════════════════════════════════════════════════════════════════╗%NC%
echo %YELLOW%║%NC%  %WHITE%1.%NC% Open %CYAN%http://127.0.0.1:%GUI_PORT%%NC% in your browser                              %YELLOW%║%NC%
echo %YELLOW%║%NC%  %WHITE%2.%NC% Go to %BOLD%Models%NC% tab → Select a model                                            %YELLOW%║%NC%
echo %YELLOW%║%NC%  %WHITE%3.%NC% Switch to %BOLD%Chat%NC% tab → Start talking!                                          %YELLOW%║%NC%
echo %YELLOW%║%NC%  %WHITE%4.%NC% Watch real-time inference with native GPU acceleration                               %YELLOW%║%NC%
echo %YELLOW%╚════════════════════════════════════════════════════════════════════════════════╝%NC%
echo.

echo %DIM%Press Ctrl+C in each console window to stop services.%NC%
echo %DIM%Logs: %TEMP%\ghostlink_*.log%NC%
echo.

REM Auto-open browser
start "" "http://127.0.0.1:%GUI_PORT%"

echo %GREEN%✓%NC% Launch complete! Ghostlink Studio is ready.
echo.
pause