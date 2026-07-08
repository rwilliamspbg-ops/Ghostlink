@echo off
setlocal enabledelayedexpansion

REM Ghostlink - Complete Auto-Launch Script (Windows)
REM Starts Ollama, backend and modern GUI automatically

title Ghostlink Studio - Auto-Launch

REM Show splash screen first
if exist "launch-splash.bat" (
    call launch-splash.bat
)

echo.
echo ================================================================================
echo [STARTING SERVICES - PLEASE WAIT]
echo ================================================================================
echo.

REM Check Node.js
where node >nul 2>nul
if errorlevel 1 (
    echo ERROR: Node.js not found
    pause
    exit /b 1
)

for /f "tokens=1" %%i in ('node -v') do (
    set NODE_VERSION=%%i
    set NODE_VERSION=!NODE_VERSION:v=!
    for /f "tokens=1 delims=." %%j in ("!NODE_VERSION!") do (
        if %%j lss 18 (
            echo ERROR: Node.js 18+ required
            pause
            exit /b 1
        )
    )
)

set SCRIPT_DIR=%CD%
echo [INFO] Script directory: !SCRIPT_DIR!
echo.

REM Check if ghostlink backend binary exists
if exist "ghostlink.exe" (
    echo [OK] Backend binary found
    set HAS_BACKEND=1
) else if exist "ghostlink-backend.exe" (
    echo [OK] Backend binary found
    set HAS_BACKEND=1
) else (
    echo [!] Backend binary not found - GUI will connect to http://127.0.0.1:8003
    set HAS_BACKEND=0
)

REM Check GUI
if not exist "ghostlink_gui_modern" (
    echo ERROR: ghostlink_gui_modern directory not found
    pause
    exit /b 1
)

cd /d "!SCRIPT_DIR!\ghostlink_gui_modern"

REM Install dependencies if needed
if not exist "node_modules" (
    echo [INFO] Installing dependencies...
    call npm install --legacy-peer-deps >nul 2>&1
)

echo.
echo ================================================================================
echo [INIT SERVICES]
echo ================================================================================
echo.

REM Start Ollama if available
set OLLAMA_PID=
where ollama >nul 2>nul
if !errorlevel! equ 0 (
    timeout /t 1 /nobreak >nul 2>&1
    for /f %%i in ('powershell -Command "try { $null = Invoke-WebRequest -Uri 'http://localhost:11434/api/tags' -ErrorAction Stop; Write-Output 'running' } catch { Write-Output 'stopped' }"') do (
        if "%%i"=="stopped" (
            echo [1] Starting Ollama...
            cd /d "!SCRIPT_DIR!"
            start "Ollama" /MIN ollama serve
            set OLLAMA_PID=1
            echo [OK] Ollama started
            echo     Log: Check Ollama window
            timeout /t 3 /nobreak >nul
        ) else (
            echo [OK] Ollama already running on http://localhost:11434
        )
    )
) else (
    echo [!] Ollama not installed
    echo     Install from: https://ollama.ai
    echo     Backend will use mock responses without real inference
)

REM Start backend if binary exists
if !HAS_BACKEND! equ 1 (
    cd /d "!SCRIPT_DIR!"
    if !OLLAMA_PID! equ 1 (
        echo [2] Starting backend...
    ) else (
        echo [1] Starting backend...
    )
    if exist "ghostlink.exe" (
        start "Ghostlink Backend" /MIN ghostlink serve 0.0.0.0 8003
    ) else if exist "ghostlink-backend.exe" (
        start "Ghostlink Backend" /MIN ghostlink-backend serve 0.0.0.0 8003
    )
    echo [OK] Backend started
    echo     Check command window
    timeout /t 2 /nobreak >nul
)

REM Start GUI
cd /d "!SCRIPT_DIR!\ghostlink_gui_modern"
if !OLLAMA_PID! equ 1 (
    if !HAS_BACKEND! equ 1 (
        echo [3] Starting GUI...
    ) else (
        echo [2] Starting GUI...
    )
) else if !HAS_BACKEND! equ 1 (
    echo [2] Starting GUI...
) else (
    echo [1] Starting GUI...
)
echo [OK] Dev server starting

REM Open browser after delay
timeout /t 5 /nobreak >nul
start http://localhost:3000

echo.
echo ================================================================================
echo [SERVICES ONLINE]
echo ================================================================================
echo.
if !OLLAMA_PID! equ 1 (
    echo   Ollama:   http://localhost:11434
)
if !HAS_BACKEND! equ 1 (
    echo   Backend:  http://127.0.0.1:8003
)
echo   GUI:      http://localhost:3000
echo.
echo   Check: http://localhost:3000
echo ================================================================================
echo.

echo Starting development server...
echo.

call npm run dev -- --host 0.0.0.0

echo.
echo [SHUTTING DOWN...]
echo All services stopped.
pause
