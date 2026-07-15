@echo off
REM Ghostlink Studio - Fast Launch (Windows)
REM Bypasses build step - just starts services.

setlocal enabledelayedexpansion

echo.
echo === Ghostlink Studio - Fast Launch (No Build) ===
echo.

set "GHOSTLINK_SKIP_BUILD=1"
call "%~dp0launch-complete.bat"
exit /b %errorlevel%
