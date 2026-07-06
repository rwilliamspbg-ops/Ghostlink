@echo off
REM Ghostlink - Unified launcher script (Windows)
REM Automatically starts backend and GUI

setlocal enabledelayedexpansion

title Ghostlink Studio - Unified Launcher

echo.
echo ========================================
echo   GHOSTLINK STUDIO
echo   Unified Launcher
echo ========================================
echo.

REM Get current directory
set SCRIPT_DIR=%~dp0

REM Check if GUI directory exists
if not exist "%SCRIPT_DIR%ghostlink_gui_modern" (
    echo ERROR: ghostlink_gui_modern directory not found
    echo Current directory: %CD%
    echo Script directory: %SCRIPT_DIR%
    pause
    exit /b 1
)

REM Change to GUI directory
cd /d "%SCRIPT_DIR%ghostlink_gui_modern"

echo [INFO] Launching Ghostlink Studio GUI...
echo [INFO] Current directory: %CD%

REM Start GUI launcher
call launch-gui.bat

pause
