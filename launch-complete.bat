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
    goto :eof
)

set SCRIPT_DIR=%~dp0
cd /d "%SCRIPT_DIR%"

echo [INFO] Script directory: %SCRIPT_DIR%

REM Check if backend exists, if not build it
if not exist "ghostlink.exe" (
    echo [INFO] Backend executable not found. Building...
    cargo build --release -p ghost-link
    copy target\release\ghost-link.exe .\ghostlink.exe
)
echo [✓] Backend executable ready

REM Check GUI
if not exist "ghostlink_gui_modern" (
    echo ERROR: ghostlink_gui_modern directory not found
    pause
    goto :eof
)

cd /d "%SCRIPT_DIR%ghostlink_gui_modern"

REM Install dependencies if needed
if not exist "node_modules" (
    echo [INFO] Installing GUI dependencies...
    call npm install --legacy-peer-deps >nul 2>&1
)

echo.
echo ================================================================================
echo   Starting Services:
echo.

REM Start backend
cd /d "%SCRIPT_DIR%"
echo [1] Starting backend...
start /B ghostlink.exe serve > ghostlink.log 2>&1
echo [✓] Backend started

REM Wait for backend health
echo [INFO] Waiting for backend to be ready...
:health_loop
set /a retry_count+=1
curl -s http://127.0.0.1:8003/health > nul
if %errorlevel% equ 0 (
    echo [✓] Backend ready!
    goto start_gui
)
if %retry_count% gtr 10 (
    echo [!] Timed out waiting for backend
    goto start_gui
)
timeout /t 1 /nobreak > nul
goto health_loop

:start_gui
REM Start GUI
cd /d "%SCRIPT_DIR%ghostlink_gui_modern"
echo [2] Starting GUI...

REM Open browser
start http://localhost:3000

echo.
echo ================================================================================
echo   Services Ready:
echo.
echo   Backend:  http://127.0.0.1:8003
echo   GUI:      http://localhost:3000
echo.
echo   Press Ctrl+C to stop all services
echo ================================================================================
echo.

REM Start dev server (foreground)
call npm run dev

pause
