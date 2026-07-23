# Ghostlink Studio - native Windows launcher (no WSL).
#
# Runs ghost-link.exe + a Vulkan-enabled llama-server.exe + the React GUI
# directly on Windows so the real Windows GPU (DirectML/Vulkan-capable
# drivers, detected via ghostlink-core::system_profile on target_os=windows)
# is actually visible to the app. launch.bat's previous WSL-only path ran a
# Linux build inside WSL2, where this hardware's GPU is invisible to both
# GPU auto-detection and llama.cpp.
#
# Mirrors the relevant parts of launch.sh's start_services(), adapted for
# native Windows tooling (MSVC/cmake/npm) instead of bash/apt.

param(
    [string]$ApiHost = "127.0.0.1",
    [int]$ApiPort = 8003,
    [int]$GuiPort = 5173,
    [int]$LlamaPort = 8080,
    [switch]$OpenBrowser,
    [switch]$SkipLlamaBuild
)

$ErrorActionPreference = "Stop"
$RootDir = $PSScriptRoot
Set-Location $RootDir

$LogDir = Join-Path $RootDir "logs"
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $RootDir "models") | Out-Null

function Write-Step($msg) { Write-Host "  > $msg" -ForegroundColor Cyan }
function Write-Ok($msg)   { Write-Host "  [ok] $msg" -ForegroundColor Green }
function Write-Warn($msg) { Write-Host "  [!] $msg" -ForegroundColor Yellow }
function Write-Err($msg)  { Write-Host "  [x] $msg" -ForegroundColor Red }

function Free-Port([int]$Port) {
    try {
        $conns = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue
        foreach ($pid_ in ($conns | Select-Object -ExpandProperty OwningProcess -Unique)) {
            if ($pid_ -and $pid_ -ne $PID) {
                Stop-Process -Id $pid_ -Force -ErrorAction SilentlyContinue
            }
        }
    } catch {
        # Get-NetTCPConnection can be unavailable/restricted in some environments; best-effort only.
    }
}

function Wait-Http([string]$Url, [string]$Label, [int]$TimeoutSec = 60) {
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        try {
            $resp = Invoke-WebRequest -Uri $Url -UseBasicParsing -TimeoutSec 3 -ErrorAction Stop
            if ($resp.StatusCode -ge 200 -and $resp.StatusCode -lt 500) {
                return $true
            }
        } catch {
            # not ready yet
        }
        Start-Sleep -Milliseconds 500
    }
    Write-Err "$Label did not become ready within ${TimeoutSec}s ($Url)"
    return $false
}

Write-Host ""
Write-Host "Ghostlink Studio - native Windows launch" -ForegroundColor White
Write-Host ""

# --- 1. Build ghost-link.exe (native Windows binary, real GPU auto-detection) ---
Write-Step "Ghost-Link API binary"
$ApiBin = Join-Path $RootDir "target\release\ghost-link.exe"
if (-not (Test-Path $ApiBin)) {
    Write-Warn "Building release binary (first run only)..."
    cargo build --release -p ghost-link
    if ($LASTEXITCODE -ne 0) { Write-Err "cargo build failed"; exit 1 }
}
Write-Ok "API binary: $ApiBin"

# --- 2. Resolve inference backend: native llama-server (default) or ollama ---
$InferenceBackend = $env:GHOSTLINK_INFERENCE_BACKEND
if ([string]::IsNullOrWhiteSpace($InferenceBackend)) { $InferenceBackend = "native" }
$InferenceBackend = $InferenceBackend.ToLowerInvariant()

$LlamaBin = $null
if ($InferenceBackend -eq "ollama") {
    Write-Step "Ollama inference (external runtime)"
    $ollamaCmd = Get-Command ollama -ErrorAction SilentlyContinue
    if (-not $ollamaCmd) {
        Write-Err "GHOSTLINK_INFERENCE_BACKEND=ollama but 'ollama' was not found on PATH."
        Write-Host "  Install Ollama, or unset GHOSTLINK_INFERENCE_BACKEND to use native llama-server." -ForegroundColor DarkGray
        exit 1
    }
    if (-not (Wait-Http "http://127.0.0.1:11434/api/tags" "Ollama" 3)) {
        Write-Warn "Starting ollama serve..."
        Start-Process -FilePath $ollamaCmd.Source -ArgumentList @("serve") -WindowStyle Hidden `
            -RedirectStandardOutput (Join-Path $LogDir "ollama.log") -RedirectStandardError (Join-Path $LogDir "ollama.err.log") | Out-Null
        if (-not (Wait-Http "http://127.0.0.1:11434/api/tags" "Ollama" 30)) {
            Write-Err "Ollama did not become ready"
            exit 1
        }
    }
    Write-Ok "Ollama ready"
} else {
    $InferenceBackend = "native"
    # --- Ensure a Vulkan-enabled llama-server.exe ---
    Write-Step "llama-server (Vulkan)"
    $LlamaCandidates = @(
        "third_party\llama.cpp\build\bin\Release\llama-server.exe",
        "third_party\llama.cpp\build\bin\llama-server.exe"
    ) | ForEach-Object { Join-Path $RootDir $_ }
    if ($env:GHOSTLINK_LLAMA_SERVER_BIN -and (Test-Path $env:GHOSTLINK_LLAMA_SERVER_BIN)) {
        $LlamaBin = $env:GHOSTLINK_LLAMA_SERVER_BIN
    } else {
        $LlamaBin = $LlamaCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
    }

    if (-not $LlamaBin -and -not $SkipLlamaBuild) {
        Write-Warn "Building llama-server with Vulkan support (first run only, several minutes)..."
        $LlamaDir = Join-Path $RootDir "third_party\llama.cpp"
        if (-not (Test-Path $LlamaDir)) {
            git clone --depth 1 https://github.com/ggml-org/llama.cpp.git $LlamaDir
        }
        Push-Location $LlamaDir
        try {
            cmake -S . -B build -DGGML_VULKAN=ON -DCMAKE_BUILD_TYPE=Release *>> (Join-Path $LogDir "llama_cmake_configure.log")
            if ($LASTEXITCODE -ne 0) { Write-Err "cmake configure failed - see logs\llama_cmake_configure.log"; exit 1 }
            cmake --build build --config Release --target llama-server -j *>> (Join-Path $LogDir "llama_cmake_build.log")
            if ($LASTEXITCODE -ne 0) { Write-Err "llama-server build failed - see logs\llama_cmake_build.log"; exit 1 }
        } finally {
            Pop-Location
        }
        $LlamaBin = $LlamaCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
    }

    if (-not $LlamaBin) {
        Write-Err "llama-server.exe not found and build was skipped/failed."
        Write-Host "  Set GHOSTLINK_LLAMA_SERVER_BIN to an existing llama-server.exe, or drop -SkipLlamaBuild." -ForegroundColor DarkGray
        exit 1
    }
    Write-Ok "llama-server: $LlamaBin"
}

# --- 3. Free stale listeners on the ports we're about to use ---
Free-Port $ApiPort
Free-Port $GuiPort
if ($InferenceBackend -eq "native") { Free-Port $LlamaPort }
Start-Sleep -Milliseconds 300

# --- 4. Start the Ghost-Link API server ---
Write-Step "Starting Ghost-Link API (port $ApiPort)"
$env:GHOSTLINK_INFERENCE_BACKEND = $InferenceBackend
if ($InferenceBackend -eq "native") {
    $env:GHOSTLINK_NATIVE_ENGINE = "llama_server"
    $env:GHOSTLINK_LLAMA_SERVER_URL = "http://127.0.0.1:$LlamaPort"
    $env:GHOSTLINK_LLAMA_SERVER_BIN = $LlamaBin
}
$logicalCores = [Environment]::ProcessorCount
$env:GHOSTLINK_LLAMA_THREADS = [Math]::Max(1, $logicalCores - 1)
$env:VITE_GHOSTLINK_API_BASE = "http://${ApiHost}:${ApiPort}"
$env:VITE_PROXY_TARGET = "http://${ApiHost}:${ApiPort}"

$apiLog = Join-Path $LogDir "ghostlink_api.log"
$apiProc = Start-Process -FilePath $ApiBin -ArgumentList @("serve", $ApiHost, $ApiPort) `
    -WorkingDirectory $RootDir -PassThru -WindowStyle Hidden `
    -RedirectStandardOutput $apiLog -RedirectStandardError (Join-Path $LogDir "ghostlink_api.err.log")

if (-not (Wait-Http "http://${ApiHost}:${ApiPort}/health" "Ghostlink API" 90)) {
    Write-Host "  Last API log lines:" -ForegroundColor DarkGray
    Get-Content $apiLog -Tail 30 -ErrorAction SilentlyContinue
    exit 1
}
Write-Ok "API ready (PID $($apiProc.Id))"

# --- 5. Start the React GUI (Vite dev server) ---
Write-Step "Starting React GUI (port $GuiPort)"
$GuiDir = Join-Path $RootDir "ghostlink_gui_modern"
Push-Location $GuiDir
try {
    if (-not (Test-Path (Join-Path $GuiDir "node_modules"))) {
        Write-Warn "Installing npm packages (first run only)..."
        npm install --legacy-peer-deps *>> (Join-Path $LogDir "ghostlink_frontend_install.log")
        if ($LASTEXITCODE -ne 0) { Write-Err "npm install failed - see logs\ghostlink_frontend_install.log"; exit 1 }
    }
    $guiLog = Join-Path $LogDir "ghostlink_frontend.log"
    $guiProc = Start-Process -FilePath "cmd.exe" `
        -ArgumentList @("/c", "npm run dev -- --host $ApiHost --port $GuiPort") `
        -WorkingDirectory $GuiDir -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput $guiLog -RedirectStandardError (Join-Path $LogDir "ghostlink_frontend.err.log")
} finally {
    Pop-Location
}

if (-not (Wait-Http "http://${ApiHost}:${GuiPort}" "React Frontend" 60)) {
    Write-Host "  Last frontend log lines:" -ForegroundColor DarkGray
    Get-Content $guiLog -Tail 30 -ErrorAction SilentlyContinue
    exit 1
}
Write-Ok "Frontend ready (PID $($guiProc.Id))"

Write-Host ""
Write-Host "Ghostlink Studio is running:" -ForegroundColor Green
Write-Host "  Web Interface -> http://${ApiHost}:${GuiPort}"
Write-Host "  API Server    -> http://${ApiHost}:${ApiPort}"
if ($InferenceBackend -eq "ollama") {
    Write-Host "  Ollama        -> http://127.0.0.1:11434"
} else {
    Write-Host "  Native Inference (llama-server) -> http://127.0.0.1:${LlamaPort}"
}
Write-Host ""
Write-Host "  API PID: $($apiProc.Id)   GUI PID: $($guiProc.Id)"
Write-Host "  Logs: $LogDir"
Write-Host ""

if ($OpenBrowser) {
    Start-Process "http://${ApiHost}:${GuiPort}"
}

# Save PIDs so a future stop script (or Ctrl+C below) can clean up.
"$($apiProc.Id)`n$($guiProc.Id)" | Set-Content (Join-Path $LogDir "native_launch.pids")

Write-Host "Press Ctrl+C to stop both services." -ForegroundColor DarkGray
try {
    while ($true) {
        Start-Sleep -Seconds 2
        if ($apiProc.HasExited) { Write-Err "API process exited unexpectedly"; break }
        if ($guiProc.HasExited) { Write-Err "GUI process exited unexpectedly"; break }
    }
} finally {
    Write-Host ""
    Write-Step "Shutting down..."
    Stop-Process -Id $apiProc.Id -Force -ErrorAction SilentlyContinue
    Stop-Process -Id $guiProc.Id -Force -ErrorAction SilentlyContinue
    Free-Port $LlamaPort
}
