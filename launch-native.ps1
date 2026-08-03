# Ghostlink Studio - native Windows launcher (no WSL).
#
# Runs ghost-link.exe + control-plane.exe (Go gateway) + a Vulkan-enabled
# llama-server.exe + the React GUI directly on Windows so the real Windows
# GPU (DirectML/Vulkan-capable drivers, detected via
# ghostlink-core::system_profile on target_os=windows) is actually visible
# to the app. launch.bat's previous WSL-only path ran a Linux build inside
# WSL2, where this hardware's GPU is invisible to both GPU auto-detection
# and llama.cpp.
#
# The GUI talks to control-plane (:8000), which proxies everything through
# to ghost-link (:8003) — the same role it already played for the
# docker-compose deployment, just now also fronting native dev instead of
# the GUI hitting ghost-link directly.
#
# Mirrors the relevant parts of launch.sh's start_services(), adapted for
# native Windows tooling (MSVC/cmake/npm/go) instead of bash/apt.

param(
    [string]$ApiHost = "127.0.0.1",
    [int]$ApiPort = 8003,
    [int]$ControlPlanePort = 8000,
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

# -SkipCertificateCheck (needed below for ghost-link's self-signed loopback
# cert when TLS is on) only exists on Invoke-WebRequest in PowerShell 6+;
# Windows PowerShell 5.1 (still the `powershell.exe` default on stock Windows)
# throws a parameter-binding error for it on every attempt, which the catch
# below swallows as "not ready" until the wait times out. Use the
# ServicePointManager callback instead when running under 5.1 - it's honored
# by 5.1's WebRequest-based Invoke-WebRequest the same way -SkipCertificateCheck
# is honored by 7's HttpClient-based one.
#
# The callback can't be a plain scriptblock: ServerCertificateValidationCallback
# fires on a .NET networking thread with no PowerShell runspace attached, so a
# scriptblock delegate throws "There is no Runspace available to run scripts in
# this thread" the instant .NET invokes it (silently, inside Invoke-WebRequest's
# own try/catch, indistinguishable from "still starting up"). A compiled
# delegate via Add-Type has no runspace dependency and works from any thread.
$IsPwshCore = $PSVersionTable.PSVersion.Major -ge 6
if (-not $IsPwshCore) {
    if (-not ("GhostlinkTrustAllCerts" -as [type])) {
        Add-Type @"
using System.Net.Security;
using System.Security.Cryptography.X509Certificates;
public class GhostlinkTrustAllCerts {
    public static bool Validate(object sender, X509Certificate cert, X509Chain chain, SslPolicyErrors errors) {
        return true;
    }
}
"@
    }
    [System.Net.ServicePointManager]::ServerCertificateValidationCallback = [Delegate]::CreateDelegate(
        [System.Net.Security.RemoteCertificateValidationCallback],
        [GhostlinkTrustAllCerts],
        "Validate"
    )
}

function Wait-Http([string]$Url, [string]$Label, [int]$TimeoutSec = 60) {
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        try {
            if ($IsPwshCore) {
                $resp = Invoke-WebRequest -Uri $Url -UseBasicParsing -TimeoutSec 3 -SkipCertificateCheck -ErrorAction Stop
            } else {
                $resp = Invoke-WebRequest -Uri $Url -UseBasicParsing -TimeoutSec 3 -ErrorAction Stop
            }
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

# ghost-link binds HTTPS instead of HTTP whenever settings.json's persisted
# "enable_tls" is true (crates/ghost-link/src/main.rs: `use_tls = settings.enable_tls
# || !is_loopback_host(host)` — host here is always loopback, so this flag alone
# decides it). That's a GUI-toggleable, sticky-across-restarts preference this
# script previously had no idea about, so it always probed http:// — which just
# hangs against an HTTPS-only listener until the readiness wait times out and the
# whole launch aborts, even though ghost-link came up fine.
function Get-ApiScheme {
    $settingsPath = Join-Path $RootDir "settings.json"
    if (Test-Path $settingsPath) {
        try {
            $settings = Get-Content $settingsPath -Raw | ConvertFrom-Json
            if ($settings.enable_tls -eq $true) { return "https" }
        } catch {
            # malformed/unreadable settings.json - fall through to the http default
        }
    }
    return "http"
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

# --- 2. Build control-plane.exe (Go gateway in front of ghost-link) ---
Write-Step "Control-plane binary"
$ControlPlaneDir = Join-Path $RootDir "control-plane"
$ControlPlaneBin = Join-Path $ControlPlaneDir "control-plane.exe"
if (-not (Test-Path $ControlPlaneBin)) {
    Write-Warn "Building control-plane binary (first run only)..."
    Push-Location $ControlPlaneDir
    try {
        go build -o control-plane.exe .
        if ($LASTEXITCODE -ne 0) { Write-Err "go build failed"; exit 1 }
    } finally {
        Pop-Location
    }
}
Write-Ok "Control-plane binary: $ControlPlaneBin"

# --- 3. Resolve inference backend: native llama-server (default) or ollama ---
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

# --- 4. Free stale listeners on the ports we're about to use ---
Free-Port $ApiPort
Free-Port $ControlPlanePort
Free-Port $GuiPort
if ($InferenceBackend -eq "native") { Free-Port $LlamaPort }
Start-Sleep -Milliseconds 300

# --- 5. Start the Ghost-Link API server ---
Write-Step "Starting Ghost-Link API (port $ApiPort)"
$env:GHOSTLINK_INFERENCE_BACKEND = $InferenceBackend
if ($InferenceBackend -eq "native") {
    $env:GHOSTLINK_NATIVE_ENGINE = "llama_server"
    $env:GHOSTLINK_LLAMA_SERVER_URL = "http://127.0.0.1:$LlamaPort"
    $env:GHOSTLINK_LLAMA_SERVER_BIN = $LlamaBin
}
$logicalCores = [Environment]::ProcessorCount
$env:GHOSTLINK_LLAMA_THREADS = [Math]::Max(1, $logicalCores - 1)

# GHOSTLINK_VRAM_GB was never set on this launch path, so native_engine.rs's
# perf-tier tables (crates/ghost-link/src/native_engine.rs:default_perf_args)
# always fell back to their worst-case "<4GB" bucket regardless of the real
# hardware -- -b 512 -ub 128, the smallest prompt micro-batch in the table,
# directly tanking prefill throughput / time-to-first-token. /api/metrics
# already detects this machine's AMD Radeon 860M at ~4GB via
# ghostlink-core::system_profile, so wire that same figure in here to move
# batch/ubatch/ctx tiering up to their ">=4GB" values instead. GHOSTLINK_LLAMA_NGL
# is pinned to -1 (offload every layer llama-server can fit, its own default)
# so this doesn't also feed the separate VRAM->ngl cap table in get_ngl(),
# which would otherwise cap offload at a fixed 12 layers -- a regression from
# the full-offload that's already working today.
$env:GHOSTLINK_VRAM_GB = "4"
$env:GHOSTLINK_LLAMA_NGL = "-1"

$ApiScheme = Get-ApiScheme
if ($ApiScheme -eq "https") {
    Write-Warn "settings.json has enable_tls=true - Ghost-Link API will bind HTTPS (self-signed cert)"
}

$apiLog = Join-Path $LogDir "ghostlink_api.log"
$apiProc = Start-Process -FilePath $ApiBin -ArgumentList @("serve", $ApiHost, $ApiPort) `
    -WorkingDirectory $RootDir -PassThru -WindowStyle Hidden `
    -RedirectStandardOutput $apiLog -RedirectStandardError (Join-Path $LogDir "ghostlink_api.err.log")

if (-not (Wait-Http "${ApiScheme}://${ApiHost}:${ApiPort}/health" "Ghostlink API" 90)) {
    Write-Host "  Last API log lines:" -ForegroundColor DarkGray
    Get-Content $apiLog -Tail 30 -ErrorAction SilentlyContinue
    exit 1
}
Write-Ok "API ready (PID $($apiProc.Id))"

# --- 6. Start the Go control-plane in front of it ---
# The GUI talks to this, not directly to ghost-link — same absolute
# cross-origin-URL pattern as before (VITE_GHOSTLINK_API_BASE/VITE_PROXY_TARGET),
# just pointed at the gateway's port instead of ghost-link's. Started after
# ghost-link is confirmed healthy since it needs a real backend to proxy to.
Write-Step "Starting control-plane (port $ControlPlanePort)"
$env:PORT = $ControlPlanePort
$env:GHOSTLINK_BACKEND_URL = "${ApiScheme}://${ApiHost}:${ApiPort}"
# control-plane's WorkingDirectory below is $ControlPlaneDir, but
# auth.LoadAPIKey() defaults to reading "api_key.txt" relative to cwd —
# and ghost-link (started above with -WorkingDirectory $RootDir) writes
# that file to $RootDir, not $ControlPlaneDir. Without this, control-plane
# silently never finds the key, which doesn't just skip its own edge auth
# check (tolerable — ghost-link's own auth still applies end to end) but
# also means requests that should be rejected for free by that edge check
# fall through and consume rate-limit budget instead, so ordinary polling
# load can trip the rate limiter far sooner than it's supposed to.
$env:GHOSTLINK_API_KEY_PATH = Join-Path $RootDir "api_key.txt"

$cpLog = Join-Path $LogDir "control_plane.log"
$cpProc = Start-Process -FilePath $ControlPlaneBin `
    -WorkingDirectory $ControlPlaneDir -PassThru -WindowStyle Hidden `
    -RedirectStandardOutput $cpLog -RedirectStandardError (Join-Path $LogDir "control_plane.err.log")

if (-not (Wait-Http "http://${ApiHost}:${ControlPlanePort}/health" "Control-plane" 30)) {
    Write-Host "  Last control-plane log lines:" -ForegroundColor DarkGray
    Get-Content $cpLog -Tail 30 -ErrorAction SilentlyContinue
    exit 1
}
Write-Ok "Control-plane ready (PID $($cpProc.Id))"

$env:VITE_GHOSTLINK_API_BASE = "http://${ApiHost}:${ControlPlanePort}"
$env:VITE_PROXY_TARGET = "http://${ApiHost}:${ControlPlanePort}"

# --- 7. Regenerate public/env-config.js with the real gateway URL ---
# Same runtime-injection mechanism the Dockerfile's entrypoint.sh uses for
# the containerized deploy (writes window._env_ from an env var at startup)
# — without this, the file's committed static default silently overrides
# the VITE_GHOSTLINK_API_BASE set above, since window._env_ wins in
# config.ts's resolveApiBase() priority order.
Write-Step "Writing env-config.js (control-plane at $($env:VITE_GHOSTLINK_API_BASE))"
$envConfigPath = Join-Path $RootDir "ghostlink_gui_modern\public\env-config.js"
@"
// Runtime environment configuration for Ghostlink Studio
// Regenerated by launch-native.ps1 on each run - do not edit by hand.
window._env_ = {
  GHOSTLINK_API_BASE: '$($env:VITE_GHOSTLINK_API_BASE)'
};
"@ | Set-Content -Path $envConfigPath -NoNewline
Write-Ok "env-config.js written"

# --- 8. Start the React GUI (Vite dev server) ---
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
Write-Host "  Web Interface  -> http://${ApiHost}:${GuiPort}"
Write-Host "  Control-plane  -> http://${ApiHost}:${ControlPlanePort}  (GUI talks to this)"
Write-Host "  API Server     -> http://${ApiHost}:${ApiPort}  (behind control-plane)"
if ($InferenceBackend -eq "ollama") {
    Write-Host "  Ollama         -> http://127.0.0.1:11434"
} else {
    Write-Host "  Native Inference (llama-server) -> http://127.0.0.1:${LlamaPort}"
}
Write-Host ""
Write-Host "  API PID: $($apiProc.Id)   Control-plane PID: $($cpProc.Id)   GUI PID: $($guiProc.Id)"
Write-Host "  Logs: $LogDir"
Write-Host ""

if ($OpenBrowser) {
    Start-Process "http://${ApiHost}:${GuiPort}"
}

# Save PIDs so a future stop script (or Ctrl+C below) can clean up.
"$($apiProc.Id)`n$($cpProc.Id)`n$($guiProc.Id)" | Set-Content (Join-Path $LogDir "native_launch.pids")

Write-Host "Press Ctrl+C to stop all services." -ForegroundColor DarkGray
try {
    while ($true) {
        Start-Sleep -Seconds 2
        if ($apiProc.HasExited) { Write-Err "API process exited unexpectedly"; break }
        if ($cpProc.HasExited) { Write-Err "Control-plane process exited unexpectedly"; break }
        if ($guiProc.HasExited) { Write-Err "GUI process exited unexpectedly"; break }
    }
} finally {
    Write-Host ""
    Write-Step "Shutting down..."
    Stop-Process -Id $apiProc.Id -Force -ErrorAction SilentlyContinue
    Stop-Process -Id $cpProc.Id -Force -ErrorAction SilentlyContinue
    Stop-Process -Id $guiProc.Id -Force -ErrorAction SilentlyContinue
    Free-Port $LlamaPort
}
