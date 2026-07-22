@echo off
REM Native llama-server is default. For Ollama: set GHOSTLINK_INFERENCE_BACKEND=ollama
if /I "%GHOSTLINK_INFERENCE_BACKEND%"=="" set "GHOSTLINK_INFERENCE_BACKEND=native"
call "%~dp0launch.bat" %*
