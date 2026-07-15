@echo off
setlocal enabledelayedexpansion
set "ROOT_DIR=%~dp0.."
cd /d "%ROOT_DIR%"
call launch-complete.bat %*
