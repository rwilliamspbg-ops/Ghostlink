@echo off
REM Ghostlink Quickstart - Windows
REM Verifies prerequisites, builds backend, downloads model, runs smoke flow.

setlocal enabledelayedexpansion
set "ROOT_DIR=%~dp0.."

echo [INFO] Starting Ghostlink quickstart in %ROOT_DIR%

REM Check cargo
cargo --version >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Missing required command: cargo
    echo FIX: Install Rust from https://rustup.rs/
    exit /b 1
)
echo [OK] Cargo verified

REM Check node
node --version >nul 2>&1
if errorlevel 1 (
    echo [WARN] Node.js not found. GUI will not build without it.
    echo FIX: Install from https://nodejs.org/
) else (
    echo [OK] Node.js verified
)

REM Check cmake
cmake --version >nul 2>&1
if errorlevel 1 (
    echo [WARN] CMake not found. llama-server build will fail without it.
    echo FIX: winget install Kitware.CMake
) else (
    echo [OK] CMake verified
)

REM Config
if not exist "%ROOT_DIR%\ghostlink.toml" (
    if exist "%ROOT_DIR%\ghostlink.example.toml" (
        copy "%ROOT_DIR%\ghostlink.example.toml" "%ROOT_DIR%\ghostlink.toml" >nul
        echo [OK] Created local config from template
    ) else (
        echo [ERROR] Missing ghostlink.example.toml
        exit /b 1
    )
) else (
    echo [INFO] Using existing ghostlink.toml
)

REM Build
echo [INFO] Building ghost-link binary...
cd /d "%ROOT_DIR%"
cargo build -p ghost-link
if errorlevel 1 (
    echo [ERROR] Build failed
    exit /b 1
)
echo [OK] Build complete

REM Download model
set "MODEL_DIR=%ROOT_DIR%\models"
set "MODEL_FILE=%MODEL_DIR%\stories15M-q4_0.gguf"
set "MODEL_URL=https://huggingface.co/ggml-org/models/resolve/main/tinyllamas/stories15M-q4_0.gguf"

if not exist "%MODEL_DIR%" mkdir "%MODEL_DIR%" 2>nul

if not exist "%MODEL_FILE%" (
    echo [INFO] Downloading bootstrap model (~15 MB)...
    curl -L --fail --progress-bar -o "%MODEL_FILE%" "%MODEL_URL%"
    if errorlevel 1 (
        echo [WARN] Model download failed -- chat will use simulated backend
    ) else (
        echo [OK] Model downloaded
    )
) else (
    echo [OK] Model already present
)

REM Smoke flow
echo [INFO] Running smoke flow...
cargo run -p ghost-link -- --config "%ROOT_DIR%\ghostlink.toml" flow
if errorlevel 1 (
    echo [ERROR] Smoke flow failed
    echo FIX: cargo run -p ghost-link -- gui-check --strict
    echo SEE: docs\TROUBLESHOOTING.md
    exit /b 1
)
echo [OK] Smoke flow completed

echo.
echo Quickstart completed.
echo.
echo Next steps:
echo   1) Launch full stack:  launch.bat
echo   2) Fast launch:        launch-fast.bat
echo   3) Run doctor:         cargo run -p ghost-link -- doctor --strict
echo   4) Read docs:          type docs\QUICKSTART.md
echo.

pause
