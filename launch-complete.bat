@echo off
REM Ghostlink - Complete Auto-Launch Script (Windows)
REM Starts backend and modern GUI automatically

setlocal enabledelayedexpansion

title Ghostlink Studio - Auto-Launch (Backend + GUI)

cls
echo.
echo ================================================================================
echo   GHOSTLINK STUDIO - Auto-Launch
echo   Backend + Modern GUI
echo ================================================================================
echo.

REM Check Node.js
where node >nul 2>nul
if %errorlevel% neq 0 (
    echo ERROR: Node.js not found
    echo Please install Node.js 18+ from https://nodejs.org/
    pause
    exit /b 1
)

set SCRIPT_DIR=%~dp0
cd /d "%SCRIPT_DIR%"

echo [INFO] Script directory: %SCRIPT_DIR%
echo.

REM Check if backend exists
if exist "ghostlink.exe" (
    set HAS_BACKEND=1
    echo [✓] Backend executable found
) else if exist "ghostlink-backend.exe" (
    set HAS_BACKEND=1
    echo [✓] Backend executable found
) else (
    set HAS_BACKEND=0
    echo [!] Backend executable not found - GUI will connect to http://127.0.0.1:8003
)

REM Check GUI
if not exist "ghostlink_gui_modern" (
    echo ERROR: ghostlink_gui_modern directory not found
    pause
    exit /b 1
)

cd /d "%SCRIPT_DIR%ghostlink_gui_modern"

REM Install dependencies if needed
if not exist "node_modules" (
    echo [INFO] Installing dependencies...
    call npm install --legacy-peer-deps >nul 2>&1
)

echo.
echo ================================================================================
echo   Starting Services:
echo.

REM Start backend if exists
if %HAS_BACKEND% equ 1 (
    cd /d "%SCRIPT_DIR%"
    echo [1] Starting backend...
    if exist "ghostlink.exe" (
        start "" ghostlink.exe serve
    ) else (
        start "" ghostlink-backend.exe
    )
    echo [✓] Backend started
    echo     Connect to: http://127.0.0.1:8003
    timeout /t 2 /nobreak > nul
)

REM Start GUI
cd /d "%SCRIPT_DIR%ghostlink_gui_modern"
echo [2] Starting GUI...
echo [✓] Dev server starting
echo.

REM Open browser
start http://localhost:3000

echo ================================================================================
echo   Services Ready:
echo.
if %HAS_BACKEND% equ 1 (
    echo   Backend:  http://127.0.0.1:8003
)
echo   GUI:      http://localhost:3000
echo.
echo   Press Ctrl+C to stop all services
echo ================================================================================
echo.

REM Start dev server (foreground)
call npm run dev

pause
