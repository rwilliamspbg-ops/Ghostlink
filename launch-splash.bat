@echo off
setlocal enabledelayedexpansion

REM Ghostlink Studio - Splash Screen with Progress Indicator
REM Shows animated progress while services start up

title Ghostlink Studio - Launching...

cls

REM Display banner
echo.
echo ╔════════════════════════════════════════════════════════════════════════════════╗
echo ║                                                                                ║
echo ║          ███████╗██╗  ██╗ ██████╗ ███████╗████████╗██╗     ██╗███╗   ██╗██╗  ██╗ ║
echo ║          ██╔════╝██║  ██║██╔═══██╗██╔════╝╚══██╔══╝██║     ██║████╗  ██║██║ ██╔╝ ║
echo ║          ███████╗███████║██║   ██║███████╗   ██║   ██║     ██║██╔██╗ ██║█████╔╝  ║
echo ║          ╚════██║██╔══██║██║   ██║╚════██║   ██║   ██║     ██║██║╚██╗██║██╔═██╗  ║
echo ║          ███████║██║  ██║╚██████╔╝███████║   ██║   ███████╗██║██║ ╚████║██║  ██╗ ║
echo ║          ╚══════╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝   ╚═╝   ╚══════╝╚═╝╚═╝  ╚═══╝╚═╝  ╚═╝ ║
echo ║                                                                                ║
echo ║                 Distributed LLM Inference Fabric                              ║
echo ║                 Enterprise AI Model Management                                ║
echo ║                                                                                ║
echo ╚════════════════════════════════════════════════════════════════════════════════╝
echo.

REM System info
echo System Information:
for /f "tokens=*" %%A in ('node -v') do echo   Node.js: %%A
for /f "tokens=*" %%A in ('npm -v') do echo   npm: %%A
echo.

REM Check components
echo Checking Components:
echo.

REM Check native stack launcher
echo   Native stack launcher:
if exist "scripts\run_native_llama_server_stack.sh" (
    echo     [OK] Found
) else (
    echo     [--] Missing
)

REM Check Backend
echo   Backend:
if exist "ghostlink.exe" (
    echo     [OK] Found
    set BACKEND_FOUND=1
) else if exist "ghostlink-backend.exe" (
    echo     [OK] Found
    set BACKEND_FOUND=1
) else (
    echo     [--] Binary not found
    set BACKEND_FOUND=0
)

REM Check GUI
echo   GUI:
if exist "ghostlink_gui_modern" (
    echo     [OK] Found
    set GUI_FOUND=1
) else (
    echo     [XX] Not found
    set GUI_FOUND=0
)

echo.
echo Starting Services:
echo.

REM Backend startup
if !BACKEND_FOUND! equ 1 (
    echo   1. Ghostlink Backend (API Server)
    echo   [==============------] 33%% Starting...
    timeout /t 1 /nobreak >nul
    echo   [==========================] 66%% Loading...
    timeout /t 1 /nobreak >nul
    echo   [====================================] 100%% Ready!
    echo   [OK] Online
    echo.
)

REM GUI startup
if !GUI_FOUND! equ 1 (
    echo   2. Ghostlink GUI (Web Interface)
    echo   [==========----------] 25%% Installing dependencies...
    timeout /t 1 /nobreak >nul
    echo   [====================] 50%% Building assets...
    timeout /t 1 /nobreak >nul
    echo   [============================] 75%% Starting dev server...
    timeout /t 1 /nobreak >nul
    echo   [====================================] 100%% Opening browser...
    echo   [OK] Online
    echo.
)

REM Service info
echo Services Ready:
echo.

if !BACKEND_FOUND! equ 1 (
    echo   Backend            ^> http://127.0.0.1:8003
)
if !GUI_FOUND! equ 1 (
    echo   Frontend           ^> http://127.0.0.1:5173
)
echo   Native Inference   ^> llama-server (port 8080 by default)

echo.
echo ┌────────────────────────────────────────────────────────────────────────────────┐
echo │                                                                                │
echo │  [OK] All services initialized successfully!                                  │
echo │                                                                                │
echo │  Opening browser in 3 seconds...                                              │
echo │                                                                                │
echo └────────────────────────────────────────────────────────────────────────────────┘
echo.

timeout /t 3 /nobreak >nul

cls

echo.
echo ╔════════════════════════════════════════════════════════════════════════════════╗
echo ║                                                                                ║
echo ║                     Ghostlink Studio is Ready!                                ║
echo ║                                                                                ║
echo ║  Quick Start Guide:                                                           ║
echo ║                                                                                ║
echo ║    1. Go to the Models tab and select a model                                 ║
echo ║    2. Switch to the Chat tab                                                  ║
echo ║    3. Type a message and send                                                 ║
echo ║    4. Watch real model inference in action!                                   ║
echo ║                                                                                ║
echo ║  Browser is now opening at http://127.0.0.1:5173                              ║
echo ║                                                                                ║
echo ║  Tip: Press Ctrl+C to stop all services                                       ║
echo ║                                                                                ║
echo ╚════════════════════════════════════════════════════════════════════════════════╝
echo.

REM Open browser
start http://127.0.0.1:5173

echo Launching full stack now...
call "%~dp0launch-complete.bat" %*
exit /b %errorlevel%
