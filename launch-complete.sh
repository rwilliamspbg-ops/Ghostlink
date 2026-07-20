#!/usr/bin/env bash
set -euo pipefail

# Ghostlink Studio - Complete Native Launch Script (Linux/macOS)
# No Docker required. Auto-detects hardware and configures llama-server optimally.

BACKEND_HOST="${GHOSTLINK_API_HOST:-127.0.0.1}"
BACKEND_PORT="${GHOSTLINK_API_PORT:-8003}"
GUI_PORT="${GUI_PORT:-5173}"
LLAMA_PORT="${GHOSTLINK_LLAMA_SERVER_PORT:-8080}"
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MODEL_DIR="${PROJECT_ROOT}/models"
MODEL_FILE="${GHOSTLINK_MODEL_FILE:-${MODEL_DIR}/stories15M-q4_0.gguf}"
MODEL_URL="https://huggingface.co/ggml-org/models/resolve/main/tinyllamas/stories15M-q4_0.gguf"

LLAMA_PID=""
API_PID=""
GUI_PID=""

# Pre-flight validation
validate_config() {
    log "Running pre-flight validation..."
    
    # Validate VITE_GHOSTLINK_API_BASE
    local api_base="${VITE_GHOSTLINK_API_BASE:-http://${BACKEND_HOST}:${BACKEND_PORT}}"
    api_base="${api_base%% }"  # Trim trailing whitespace
    api_base="${api_base## }"  # Trim leading whitespace
    
    if [[ ! "$api_base" =~ ^https?://[^[:space:]]+$ ]]; then
        printf '[launch-complete] ERROR: Invalid API base URL: "%s"\n' "$api_base" >&2
        printf '[launch-complete] Must be a valid http:// or https:// URL without whitespace\n' >&2
        return 1
    fi
    export VITE_GHOSTLINK_API_BASE="$api_base"
    log "API base validated: $VITE_GHOSTLINK_API_BASE"
    
    # Validate required directories
    if [[ ! -d "$PROJECT_ROOT/ghostlink_gui_modern" ]]; then
        printf '[launch-complete] ERROR: GUI directory not found: %s/ghostlink_gui_modern\n' "$PROJECT_ROOT" >&2
        return 1
    fi
    
    # Validate required commands
    for cmd in cargo node curl; do
        if ! command -v "$cmd" >/dev/null 2>&1; then
            printf '[launch-complete] ERROR: Required command not found: %s\n' "$cmd" >&2
            return 1
        fi
    done
    
    log "Pre-flight validation passed"
    return 0
}

log() { printf '[launch-complete] %s\n' "$*"; }
warn() { printf '[launch-complete] WARN: %s\n' "$*" >&2; }

# Download official prebuilt llama.cpp binaries when building locally is not
# possible (no cmake, missing submodule, or a failed build). Linux x86_64 only.
# Without this, machines with no build toolchain silently drop to simulated
# mode and every chat fails with "could not connect to 127.0.0.1:8080".
download_prebuilt_llama() {
    local variant
    case "$(uname -s)/$(uname -m)" in
        Linux/x86_64)
            # The Vulkan build drives NVIDIA/AMD/Intel GPUs through the system
            # Vulkan loader without needing a CUDA/ROCm toolchain installed.
            variant="ubuntu-x64"
            if ldconfig -p 2>/dev/null | grep -q "libvulkan.so.1"; then
                variant="ubuntu-vulkan-x64"
            fi
            ;;
        Darwin/arm64)
            # Apple Silicon: official build ships with Metal GPU support.
            variant="macos-arm64"
            ;;
        Darwin/x86_64)
            variant="macos-x64"
            ;;
        *)
            warn "prebuilt llama.cpp fallback supports Linux x86_64 and macOS only"
            return 1
            ;;
    esac
    log "llama-server: resolving official prebuilt binaries ($variant)..."
    # Releases are listed newest-first; not every release publishes all
    # assets, so scan a few and take the newest matching one.
    local url
    url=$(curl -fsSL "https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=10" 2>/dev/null \
        | grep -o "https://github.com/ggml-org/llama.cpp/releases/download/[^\"]*-bin-${variant}\.tar\.gz" \
        | head -1)
    if [[ -z "$url" ]]; then
        warn "could not resolve a prebuilt llama.cpp release asset"
        return 1
    fi
    local dest="$PROJECT_ROOT/third_party/llama.cpp/build/bin"
    local tmp_tar
    tmp_tar=$(mktemp) || return 1
    log "llama-server: downloading $url"
    if ! curl -fsSL --retry 3 -o "$tmp_tar" "$url"; then
        rm -f "$tmp_tar"
        warn "prebuilt llama.cpp download failed"
        return 1
    fi
    mkdir -p "$dest"
    if ! tar xzf "$tmp_tar" -C "$dest" --strip-components=1; then
        rm -f "$tmp_tar"
        warn "prebuilt llama.cpp archive extraction failed"
        return 1
    fi
    rm -f "$tmp_tar"
    if [[ ! -f "$dest/llama-server" ]]; then
        warn "prebuilt archive did not contain llama-server"
        return 1
    fi
    chmod +x "$dest/llama-server" 2>/dev/null || true
    log "llama-server: prebuilt $variant installed to $dest"
}

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        printf '[launch-complete] missing required command: %s\n' "$1" >&2
        exit 1
    fi
}

wait_for_http() {
    local url="$1" label="$2" timeout_s="${3:-60}" start
    start=$(date +%s)
    while true; do
        if curl -sf "$url" >/dev/null 2>&1; then
            log "$label is healthy at $url"
            return 0
        fi
        if (( $(date +%s) - start >= timeout_s )); then
            printf '[launch-complete] %s failed health check: %s\n' "$label" "$url" >&2
            return 1
        fi
        sleep 1
    done
}

cleanup() {
    log "stopping services"
    [[ -n "$GUI_PID" ]] && kill "$GUI_PID" 2>/dev/null || true
    [[ -n "$API_PID" ]] && kill "$API_PID" 2>/dev/null || true
    [[ -n "$LLAMA_PID" ]] && kill "$LLAMA_PID" 2>/dev/null || true
    wait 2>/dev/null || true
}

trap cleanup EXIT INT TERM

# ==================== Hardware Detection ====================
detect_hardware() {
    echo ""
    echo "====== Ghostlink Hardware Detection ======"
    echo ""

    # CPU cores
    if [[ "$OSTYPE" == "darwin"* ]]; then
        CPU_CORES=$(sysctl -n hw.logicalcpu 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)
    else
        CPU_CORES=$(nproc 2>/dev/null || grep -c ^processor /proc/cpuinfo 2>/dev/null || echo 4)
    fi
    echo "  CPU Cores: $CPU_CORES"

    # System RAM
    if [[ "$OSTYPE" == "darwin"* ]]; then
        RAM_GB=$(sysctl -n hw.memsize 2>/dev/null | awk '{printf "%.0f", $1/1073741824}' || echo 4)
    else
        RAM_GB=$(awk '/MemTotal/ {printf "%.0f", $2/1048576}' /proc/meminfo 2>/dev/null || echo 4)
    fi
    echo "  System RAM: ${RAM_GB} GB"

    GPU_VENDOR=""
    GPU_NAME=""
    VRAM_GB=0
    BACKEND="cpu"
    NPU_DETECTED=""

    # NVIDIA GPU (CUDA)
    if command -v nvidia-smi >/dev/null 2>&1; then
        local nvidia_info
        nvidia_info=$(nvidia-smi --query-gpu=name,memory.total --format=csv,noheader,nounits 2>/dev/null | head -1)
        if [[ -n "$nvidia_info" ]]; then
            GPU_NAME=$(echo "$nvidia_info" | awk -F', ' '{print $1}')
            local vram_mib
            vram_mib=$(echo "$nvidia_info" | awk -F', ' '{print $2}')
            VRAM_GB=$(( vram_mib / 1024 ))
            GPU_VENDOR="nvidia"
            BACKEND="cuda"
            echo "  [CUDA] NVIDIA GPU: $GPU_NAME"
            echo "  VRAM: ${VRAM_GB} GB (${vram_mib} MiB)"
        fi
    fi

    # AMD GPU (ROCm)
    if [[ -z "$GPU_VENDOR" ]] && command -v rocm-smi >/dev/null 2>&1; then
        local amd_info
        amd_info=$(rocm-smi --showproductname 2>/dev/null | grep "Card model:" | head -1)
        if [[ -n "$amd_info" ]]; then
            GPU_NAME=$(echo "$amd_info" | awk -F': ' '{print $3}')
            GPU_VENDOR="amd"
            BACKEND="rocm"
            echo "  [ROCm] AMD GPU: $GPU_NAME"
        fi
    fi

    # AMD/Intel GPU via lspci (Linux)
    if [[ -z "$GPU_VENDOR" ]] && command -v lspci >/dev/null 2>&1; then
        local gpu_line
        gpu_line=$(lspci -mm 2>/dev/null | grep -iE "VGA compatible|3D controller" | head -1)
        if [[ -n "$gpu_line" ]]; then
            GPU_NAME=$(echo "$gpu_line" | grep -iEo '"([^"]*)"' | tail -1 | tr -d '"')
            if echo "$GPU_NAME" | grep -qiE "amd|radeon|advanced micro"; then
                GPU_VENDOR="amd"
                BACKEND="rocm"
                echo "  [ROCm] AMD GPU: $GPU_NAME"
            elif echo "$GPU_NAME" | grep -qiE "intel|arc|iris|uhd"; then
                GPU_VENDOR="intel"
                BACKEND="vulkan"
                echo "  [Vulkan] Intel GPU: $GPU_NAME"
            else
                GPU_VENDOR="other"
                BACKEND="vulkan"
                echo "  [Vulkan] GPU: $GPU_NAME"
            fi
        fi
    fi

    # Apple Metal (macOS)
    if [[ -z "$GPU_VENDOR" ]] && [[ "$OSTYPE" == "darwin"* ]]; then
        local metal_info
        metal_info=$(system_profiler SPDisplaysDataType 2>/dev/null | grep "Chipset Model:" | head -1 | awk -F': ' '{print $2}')
        if [[ -n "$metal_info" ]]; then
            GPU_NAME="$metal_info"
            GPU_VENDOR="apple"
            BACKEND="metal"
            echo "  [Metal] Apple GPU: $GPU_NAME"
        fi
    fi

    # NPU detection (Linux)
    if [[ -d "/sys/class/accel" ]]; then
        for accel in /sys/class/accel/*/device; do
            if [[ -f "$accel/product_name" ]]; then
                local npu_name
                npu_name=$(cat "$accel/product_name" 2>/dev/null)
                if [[ -n "$npu_name" ]]; then
                    NPU_DETECTED="$npu_name"
                    echo "  [NPU] $npu_name"
                fi
            fi
        done
    fi

    # Fallback CPU
    if [[ -z "$GPU_VENDOR" ]]; then
        echo "  No GPU detected - using CPU mode"
        BACKEND="cpu"
    fi

    echo ""
}

# ==================== Resource Planning ====================
plan_resources() {
    echo "====== Resource Planning ======"
    echo ""

    # Threads
    if [[ -n "${GHOSTLINK_LLAMA_THREADS:-}" ]]; then
        THREADS=$GHOSTLINK_LLAMA_THREADS
    else
        THREADS=$(( CPU_CORES > 1 ? CPU_CORES - 1 : 1 ))
    fi
    echo "  Threads: $THREADS (of $CPU_CORES cores)"

    # Backend override
    if [[ -n "${GHOSTLINK_LLAMA_BACKEND:-}" ]]; then
        BACKEND="$GHOSTLINK_LLAMA_BACKEND"
        log "Manual backend override: $BACKEND"
    fi

    # GPU Layers
    BACKEND_FLAGS=""
    if [[ -n "${GHOSTLINK_LLAMA_NGL:-}" ]]; then
        LLAMA_NGL=$GHOSTLINK_LLAMA_NGL
        echo "  GPU Layers: $LLAMA_NGL (user override)"
    elif [[ "$BACKEND" == "cpu" ]]; then
        LLAMA_NGL=0
        echo "  GPU Layers: 0 (CPU mode)"
    else
        if (( VRAM_GB >= 12 )); then
            LLAMA_NGL=40
            echo "  GPU Layers: 40 (VRAM-based, ${VRAM_GB} GB)"
        elif (( VRAM_GB >= 8 )); then
            LLAMA_NGL=24
            echo "  GPU Layers: 24 (VRAM-based, ${VRAM_GB} GB)"
        elif (( VRAM_GB >= 4 )); then
            LLAMA_NGL=99
            echo "  GPU Layers: 99 (full offload, ${VRAM_GB} GB)"
        else
            LLAMA_NGL=99
            echo "  GPU Layers: 99 (full offload, ${VRAM_GB} GB)"
        fi
    fi

    # Backend flags
    case "$BACKEND" in
        cuda)     BACKEND_FLAGS="" ;;
        vulkan)   BACKEND_FLAGS="--vulkan" ;;
        rocm)     BACKEND_FLAGS="" ;;
        metal)    BACKEND_FLAGS="--metal" ;;
        directml) BACKEND_FLAGS="--directml" ;;
        cpu)      BACKEND_FLAGS="" ;;
        *)        BACKEND_FLAGS="" ;;
    esac
    echo "  Backend: $(echo $BACKEND | tr '[:lower:]' '[:upper:]')"

    # Memory lock
    MLOCK_FLAG=""
    if (( RAM_GB >= 8 )); then
        MLOCK_FLAG="--mlock"
    fi

    echo ""
}

# ==================== Main ====================
main() {
    require_cmd bash
    require_cmd cargo
    require_cmd node
    require_cmd curl

    cd "$PROJECT_ROOT"
    mkdir -p "$MODEL_DIR"

    detect_hardware
    plan_resources

    # Model download
    if [[ "${GHOSTLINK_SKIP_MODEL:-0}" != "1" ]] && [[ ! -f "$MODEL_FILE" ]]; then
        log "trying to download default model to $MODEL_FILE"
        curl -L --fail -o "$MODEL_FILE" "$MODEL_URL" || {
            log "WARNING: model download failed; starting without it (use Hugging Face tab to download)"
        }
    fi

    # Find llama-server binary (check pre-built cache first, then build dirs)
    local LLAMA_SERVER_BIN=""
    for candidate in \
        "bin/llama-server" \
        "third_party/llama.cpp/build/bin/llama-server" \
        "third_party/llama.cpp/build/bin/Release/llama-server.exe" \
        "third_party/llama.cpp/build/bin/llama-server.exe"; do
        if [[ -f "$PROJECT_ROOT/$candidate" ]]; then
            LLAMA_SERVER_BIN="$PROJECT_ROOT/$candidate"
            break
        fi
    done

    # Build if missing
    if [[ -z "$LLAMA_SERVER_BIN" ]] && [[ "${GHOSTLINK_SKIP_BUILD:-0}" != "1" ]]; then
        log "llama-server binary not found, attempting build..."
        if command -v cmake >/dev/null 2>&1; then
            local LLAMA_DIR="$PROJECT_ROOT/third_party/llama.cpp"
            if [[ -d "$LLAMA_DIR" ]]; then
                pushd "$LLAMA_DIR" >/dev/null
                mkdir -p build && pushd build >/dev/null
                local CMAKE_FLAGS="-DCMAKE_BUILD_TYPE=Release"
                case "$BACKEND" in
                    cuda)     CMAKE_FLAGS="$CMAKE_FLAGS -DLLAMA_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES=all" ;;
                    vulkan)   CMAKE_FLAGS="$CMAKE_FLAGS -DLLAMA_VULKAN=ON" ;;
                    rocm)     CMAKE_FLAGS="$CMAKE_FLAGS -DLLAMA_HIPBLAS=ON -DAMDGPU_TARGETS=all" ;;
                    metal)    CMAKE_FLAGS="$CMAKE_FLAGS -DLLAMA_METAL=ON" ;;
                esac
                log "Building llama-server with: $CMAKE_FLAGS"
                cmake .. $CMAKE_FLAGS && cmake --build . --config Release --target llama-server -j 2
                popd >/dev/null
                popd >/dev/null
                # Re-check for binary
                for candidate in \
                    "bin/llama-server" \
                    "third_party/llama.cpp/build/bin/llama-server" \
                    "third_party/llama.cpp/build/bin/Release/llama-server.exe"; do
                    if [[ -f "$PROJECT_ROOT/$candidate" ]]; then
                        LLAMA_SERVER_BIN="$PROJECT_ROOT/$candidate"
                        break
                    fi
                done
            else
                warn "llama.cpp source not found at $LLAMA_DIR"
                warn "Run: git submodule update --init --recursive"
            fi
        else
            warn "cmake not found - cannot build llama-server"
        fi
    fi

    # Fallback: official prebuilt binaries when the build was not possible
    # (no cmake, submodule not checked out, or the build failed)
    if [[ -z "$LLAMA_SERVER_BIN" ]] && [[ "${GHOSTLINK_SKIP_BUILD:-0}" != "1" ]]; then
        if download_prebuilt_llama; then
            if [[ -f "$PROJECT_ROOT/third_party/llama.cpp/build/bin/llama-server" ]]; then
                LLAMA_SERVER_BIN="$PROJECT_ROOT/third_party/llama.cpp/build/bin/llama-server"
            fi
        fi
    fi

    local NATIVE_ENGINE="simulated"

    if [[ -n "$LLAMA_SERVER_BIN" ]]; then
        log "starting llama-server on port $LLAMA_PORT (ngl=$LLAMA_NGL, backend=$BACKEND)"
        "$LLAMA_SERVER_BIN" \
            -m "$MODEL_FILE" \
            --host 127.0.0.1 --port "$LLAMA_PORT" \
            -ngl "$LLAMA_NGL" \
            -np 1 \
            -t "$THREADS" \
            $MLOCK_FLAG \
            $BACKEND_FLAGS \
            >/tmp/ghostlink_llama_server.log 2>&1 &
        LLAMA_PID=$!
        wait_for_http "http://127.0.0.1:${LLAMA_PORT}/health" "llama-server" 60
        NATIVE_ENGINE="llama_server"
    else
        warn "llama-server binary not found, using simulated mode"
    fi

    export VITE_GHOSTLINK_API_BASE="http://${BACKEND_HOST}:${BACKEND_PORT}"

    log "starting Ghostlink API on port $BACKEND_PORT"
    GHOSTLINK_INFERENCE_BACKEND=native \
    GHOSTLINK_NATIVE_ENGINE="$NATIVE_ENGINE" \
    GHOSTLINK_LLAMA_SERVER_URL="http://127.0.0.1:${LLAMA_PORT}/completion" \
    GHOSTLINK_LLAMA_NGL="$LLAMA_NGL" \
    VITE_GHOSTLINK_API_BASE="$VITE_GHOSTLINK_API_BASE" \
    cargo run -p ghost-link -- serve "$BACKEND_HOST" "$BACKEND_PORT" \
        >/tmp/ghostlink_api.log 2>&1 &
    API_PID=$!
    wait_for_http "http://${BACKEND_HOST}:${BACKEND_PORT}/health" "Backend" 80
    
    # Wait for API endpoint to be fully ready
    log "waiting for API endpoint..."
    wait_for_http "http://${BACKEND_HOST}:${BACKEND_PORT}/api/health" "API endpoint" 30
    wait_for_http "http://${BACKEND_HOST}:${BACKEND_PORT}/api/health" "Backend API" 30

    log "starting GUI on port $GUI_PORT"
    cd "$PROJECT_ROOT/ghostlink_gui_modern"
    if [[ ! -d node_modules ]]; then
        npm install --legacy-peer-deps
    fi
    npm run dev -- --host 127.0.0.1 --port "$GUI_PORT" >/tmp/ghostlink_frontend.log 2>&1 &
    GUI_PID=$!
    cd "$PROJECT_ROOT"
    wait_for_http "http://127.0.0.1:${GUI_PORT}" "Frontend" 80

    echo ""
    echo "====== Ghostlink Studio is Ready! ======"
    echo ""
    echo "  Backend:  http://${BACKEND_HOST}:${BACKEND_PORT}"
    echo "  Frontend: http://127.0.0.1:${GUI_PORT}"
    echo "  Inference: http://127.0.0.1:${LLAMA_PORT} (llama-server)"
    echo ""
    echo "  Hardware: ${GPU_NAME:-CPU} (${BACKEND}, ${LLAMA_NGL} GPU layers)"
    echo "  Threads: ${THREADS}"
    [[ -n "$NPU_DETECTED" ]] && echo "  NPU: $NPU_DETECTED"
    echo ""
    log "logs: /tmp/ghostlink_*.log"

    wait
}

main "$@"
