@echo off
setlocal enabledelayedexpansion

:: Ghostlink Studio - Auto-Launch with Modern GUI (Windows)
:: This script starts the backend and automatically opens the modern web GUI

title Ghostlink Studio

echo.
echo ================================================================================
echo   GHOSTLINK STUDIO - Advanced AI Model Management
echo ================================================================================
echo.

:: Check if Node.js is installed
where node >nul 2>nul
if %errorlevel% neq 0 (
    echo ERROR: Node.js is not installed or not in PATH
    echo Please install Node.js 18+ from https://nodejs.org/
    pause
    exit /b 1
)

:: Get the directory of this script
set SCRIPT_DIR=%~dp0
cd /d "%SCRIPT_DIR%"

:: Default backend URL
set BACKEND_HOST=127.0.0.1
set BACKEND_PORT=8003
set BACKEND_URL=http://%BACKEND_HOST%:%BACKEND_PORT%
set GUI_PORT=3000
set GUI_URL=http://localhost:%GUI_PORT%

:: Parse command line arguments
if not "%1"=="" set BACKEND_URL=%1

echo [INFO] Starting Ghostlink Studio components...
echo [INFO] Backend URL: %BACKEND_URL%
echo [INFO] GUI URL: %GUI_URL%
echo.

:: Check if GUI directory exists
if not exist "ghostlink_gui_modern" (
    echo ERROR: ghostlink_gui_modern directory not found
    echo Please ensure the GUI is set up correctly
    pause
    exit /b 1
)

echo [INFO] Checking GUI dependencies...
cd ghostlink_gui_modern
if not exist "node_modules" (
    echo [INFO] Installing GUI dependencies...
    call npm install --legacy-peer-deps
    if %errorlevel% neq 0 (
        echo ERROR: Failed to install dependencies
        pause
        exit /b 1
    )
)

echo [INFO] Starting development server...
echo.
echo ================================================================================
echo   GUI will open automatically in your default browser
echo   Server running at: %GUI_URL%
echo   Backend connected to: %BACKEND_URL%
echo   Press Ctrl+C to stop
echo ================================================================================
echo.

:: Start the development server
start http://localhost:%GUI_PORT%
call npm run dev

pause
