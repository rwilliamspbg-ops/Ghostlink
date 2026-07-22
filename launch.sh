#!/usr/bin/env bash

# Ghostlink Studio - GPU-Accelerated Launch (Linux/macOS)
# Optimized for AMD ROCm with automatic GPU environment configuration

# Hardware globals (set by detect_gpu / start_services)
GPU_VENDOR=""
BACKEND="cpu"
VRAM_GB=0
NPU_DETECTED=""

# GPU Environment Variables (for AMD ROCm - ROCm.txt gfx906 mapping)
export OLLAMA_HOST=${OLLAMA_HOST:-127.0.0.1:11434}
export OLLAMA_NUM_THREAD=${OLLAMA_NUM_THREAD:-16}
export OLLAMA_GPU_MEMORY=${OLLAMA_GPU_MEMORY:-3276}
export HIP_PLATFORM=${HIP_PLATFORM:-amd}
export HSA_OVERRIDE_GFX_VERSION=${HSA_OVERRIDE_GFX_VERSION:-gfx906}
export OLLAMA_IGPU_ENABLE=${OLLAMA_IGPU_ENABLE:-1}
export OLLAMA_BATCH_SIZE=${OLLAMA_BATCH_SIZE:-512}
export OLLAMA_CACHE_SIZE=${OLLAMA_CACHE_SIZE:-2048}

# Color palette
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
WHITE='\033[1;37m'
GRAY='\033[0;37m'
DIM='\033[2m'
BOLD='\033[1m'
NC='\033[0m'
CLEAR_LINE='\033[2K\r'

# Cursor control
HIDE_CURSOR='\033[?25l'
SHOW_CURSOR='\033[?25h'

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Ensure Rust toolchain is available even when shell PATH hasn't loaded ~/.cargo/env.
ensure_rust_toolchain() {
    if command -v cargo >/dev/null 2>&1 && command -v rustc >/dev/null 2>&1; then
        return 0
    fi

    if [ -f "$HOME/.cargo/env" ]; then
        # shellcheck source=/dev/null
        . "$HOME/.cargo/env"
    fi

    if command -v cargo >/dev/null 2>&1 && command -v rustc >/dev/null 2>&1; then
        return 0
    fi

    echo -e "${RED}✗${NC} ${BOLD}Rust toolchain not found${NC} - cargo/rustc are required"
    echo -e "  Install with: ${CYAN}curl https://sh.rustup.rs -sSf | sh -s -- -y${NC}"
    echo -e "  Then restart shell or run: ${CYAN}source \"\$HOME/.cargo/env\"${NC}"
    return 1
}

# Trap to ensure cursor is shown on exit
trap 'echo -e "${SHOW_CURSOR}"; tput cnorm 2>/dev/null; exit' EXIT INT TERM

# Hide cursor for cinematic effect
echo -e "${HIDE_CURSOR}"

# Progress bar with smooth animation
progress_bar() {
    local current=$1
    local total=$2
    local width=50
    local percent=$((current * 100 / total))
    local filled=$((current * width / total))
    
    printf "  ${CYAN}[${NC}"
    printf "${GREEN}%${filled}s${NC}" | tr ' ' '█'
    printf "${DIM}%$((width - filled))s${NC}" | tr ' ' '░'
    printf "${CYAN}]${NC} ${WHITE}%3d%%${NC}" "$percent"
}

# Spinner animation
spinner() {
    local pid=$1
    local label=$2
    local spin='⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏'
    local i=0
    while kill -0 $pid 2>/dev/null; do
        printf "\r  ${CYAN}%s${NC} %s" "${spin:i++%${#spin}:1}" "$label"
        sleep 0.1
    done
    wait $pid
    return $?
}

# Wait for HTTP endpoint with animated spinner
wait_for_http() {
    local url="$1"
    local label="$2"
    local timeout_s="${3:-120}"
    local start
    start=$(date +%s)
    
    while true; do
        if curl -sf "$url" >/dev/null 2>&1; then
            printf "\r${CLEAR_LINE}  ${GREEN}✓${NC} ${WHITE}%s${NC} ${GREEN}ready${NC} at ${CYAN}%s${NC}\n" "$label" "$url"
            return 0
        fi
        if (( $(date +%s) - start >= timeout_s )); then
            printf "\r${CLEAR_LINE}  ${RED}✗${NC} ${WHITE}%s${NC} ${RED}timeout${NC} after %ds: %s\n" "$label" "$timeout_s" "$url"
            return 1
        fi
        printf "\r  ${CYAN}⠋${NC} ${WHITE}%s${NC} ${DIM}waiting...${NC}" "$label"
        sleep 0.5
        printf "\r  ${CYAN}⠙${NC} ${WHITE}%s${NC} ${DIM}waiting...${NC}" "$label"
        sleep 0.5
        printf "\r  ${CYAN}⠹${NC} ${WHITE}%s${NC} ${DIM}waiting...${NC}" "$label"
        sleep 0.5
        printf "\r  ${CYAN}⠸${NC} ${WHITE}%s${NC} ${DIM}waiting...${NC}" "$label"
        sleep 0.5
    done
}

# Animated typing effect
type_text() {
    local text="$1"
    local delay="${2:-0.02}"
    for (( i=0; i<${#text}; i++ )); do
        printf "%s" "${text:$i:1}"
        sleep $delay
    done
    echo
}

# Cinematic banner with fade-in
show_banner() {
    clear
    echo -e "${CYAN}"
    cat << 'EOF'
    ╔═════════════════════════════════════════════════════════════════════════════════════════╗
    ║                                                                                     ║
    ║     ██████╗ ██╗  ██╗ ██████╗ ████████╗███████╗██╗  ██╗████████╗ ██████╗ ██████╗    ║
    ║     ██╔══██╗██║  ██║██╔═══██╗╚══██╔══╝██╔════╝██║  ██║╚══██╔══╝██╔═══██╗██╔══██╗   ║
    ║     ██████╔╝███████║██║   ██║   ██║   ███████╗███████║   ██║   ██║   ██║██████╔╝   ║
    ║     ██╔═══╝ ██╔══██║██║   ██║   ██║   ╚════██║██╔══██║   ██║   ██║   ██║██╔══██╗   ║
    ║     ██║     ██║  ██║╚██████╔╝   ██║   ███████║██║  ██║   ██║   ╚██████╔╝██║  ██║   ║
    ║     ╚═╝     ╚═╝  ╚═╝ ╚═════╝    ╚═╝   ╚══════╝╚═╝  ╚═╝   ╚═╝    ╚═════╝ ╚═╝  ╚═╝   ║
    ║                                                                                     ║
    ║          ████████╗███████╗ █████╗ ██████╗ ███████╗██████╗ ███████╗                 ║
    ║          ╚══██╔══╝██╔════╝██╔══██╗██╔══██╗██╔════╝██╔══██╗██╔════╝                 ║
    ║             ██║   █████╗  ███████║██████╔╝█████╗  ██████╔╝███████╗                 ║
    ║             ██║   ██╔══╝  ██╔══██║██╔══██╗██╔══╝  ██╔══██╗╚════██║                 ║
    ║             ██║   ███████╗██║  ██║██║  ██║███████╗██║  ██║███████╗                 ║
    ║             ╚═╝   ╚══════╝╚═╝  ╚═╝╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝╚══════╝                 ║
    ║                                                                                     ║
    ╚════════════════════════════════════════════════════════════════════════════════════════╝
EOF
    echo -e "${NC}"
}

# System info panel
show_system_info() {
echo -e "${BLUE}┌─ System Information ─────────────────────────────────────────────────────────────┐${NC}"
echo -e "${BLUE}│${NC}  ${WHITE}OS:${NC}           $(uname -s) $(uname -r) ($(uname -m))"
echo -e "${BLUE}│${NC}  ${WHITE}Shell:${NC}         $SHELL"
echo -e "${BLUE}│${NC}  ${WHITE}Rust:${NC}          $(rustc --version 2>/dev/null | cut -d' ' -f2 || echo 'not installed')"
echo -e "${BLUE}│${NC}  ${WHITE}Cargo:${NC}         $(cargo --version 2>/dev/null | cut -d' ' -f2 || echo 'not installed')"
echo -e "${BLUE}│${NC}  ${WHITE}Node.js:${NC}       $(node -v 2>/dev/null || echo 'not installed')"
echo -e "${BLUE}│${NC}  ${WHITE}npm:${NC}           $(npm -v 2>/dev/null || echo 'not installed')"
echo -e "${BLUE}│${NC}  ${WHITE}GPU:${NC}           $(nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null | head -1 || echo 'CPU only / not detected')"
if [[ -n "$NPU_DETECTED" ]]; then
    echo -e "${BLUE}│${NC}  ${WHITE}NPU:${NC}           $NPU_DETECTED"
fi
echo -e "${BLUE}└────────────────────────────────────────────────────────────────────────────────────┘${NC}"
echo ""
}

# GPU detection
detect_gpu() {
    local gpu_name=""
    local gpu_vendor=""
    local vram_mib=0
    
    # NVIDIA CUDA
    if command -v nvidia-smi >/dev/null 2>&1; then
        gpu_name=$(nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null | head -1)
        if [ -n "$gpu_name" ]; then
            vram_mib=$(nvidia-smi --query-gpu=memory.total --format=csv,noheader,nounits 2>/dev/null | head -1)
            local vram_gb=$(( vram_mib / 1024 ))
            echo -e "  ${GREEN}╡${NC} ${WHITE}GPU${NC}          ${GREEN}NVIDIA: $gpu_name (${vram_gb} GB)${NC}"
            gpu_vendor="nvidia"
            GPU_VENDOR="nvidia"
            BACKEND="cuda"
            VRAM_GB=$vram_gb
        fi
    fi
    
    # AMD ROCm
    if [ -z "$gpu_vendor" ] && command -v rocm-smi >/dev/null 2>&1; then
        gpu_name=$(rocm-smi --showproductname 2>/dev/null | grep "Card model:" | head -1 | awk -F': ' '{print $3}')
        if [ -n "$gpu_name" ]; then
            echo -e "  ${GREEN}╡${NC} ${WHITE}GPU${NC}          ${GREEN}AMD ROCm: $gpu_name (gfx906 mapping)${NC}"
            gpu_vendor="amd"
            GPU_VENDOR="amd"
            BACKEND="rocm"
        else
            echo -e "  ${GREEN}╡${NC} ${WHITE}GPU${NC}          ${GREEN}AMD ROCm detected (gfx906 mapping)${NC}"
            gpu_vendor="amd"
            GPU_VENDOR="amd"
            BACKEND="rocm"
        fi
    fi
    
    # AMD/Intel via lspci (Linux)
    if [ -z "$gpu_vendor" ] && command -v lspci >/dev/null 2>&1; then
        local gpu_line
        gpu_line=$(lspci -mm 2>/dev/null | grep -iE "(VGA compatible|3D controller)" | head -1)
        if [ -n "$gpu_line" ]; then
            gpu_name=$(echo "$gpu_line" | grep -iEo '"([^"]*)"' | tail -1 | tr -d '"')
            if echo "$gpu_name" | grep -qiE "amd|radeon|advanced micro"; then
                echo -e "  ${GREEN}╡${NC} ${WHITE}GPU${NC}          ${GREEN}AMD: ${gpu_name:-AMD GPU}${NC}"
                gpu_vendor="amd"
                GPU_VENDOR="amd"
                BACKEND="rocm"
            elif echo "$gpu_name" | grep -qiE "intel|arc|iris|uhd"; then
                echo -e "  ${GREEN}╡${NC} ${WHITE}GPU${NC}          ${GREEN}Intel: ${gpu_name}${NC}"
                gpu_vendor="intel"
                GPU_VENDOR="intel"
                BACKEND="vulkan"
            else
                echo -e "  ${GREEN}╡${NC} ${WHITE}GPU${NC}          ${GREEN}GPU: ${gpu_name}${NC}"
                gpu_vendor="other"
                GPU_VENDOR="other"
                BACKEND="vulkan"
            fi
        fi
    fi
    
    # Apple Metal (macOS)
    if [ -z "$gpu_vendor" ] && [ "$(uname -s)" = "Darwin" ]; then
        gpu_name=$(system_profiler SPDisplaysDataType 2>/dev/null | grep "Chipset Model:" | head -1 | awk -F': ' '{print $2}')
        if [ -n "$gpu_name" ]; then
            echo -e "  ${GREEN}╡${NC} ${WHITE}GPU${NC}          ${GREEN}Apple: $gpu_name${NC}"
            gpu_vendor="apple"
            GPU_VENDOR="apple"
            BACKEND="metal"
        fi
    fi
    
    # NPU detection
    if [ -d "/sys/class/accel" ]; then
        for accel in /sys/class/accel/*/device; do
            if [ -f "$accel/product_name" ]; then
                local npu_name
                npu_name=$(cat "$accel/product_name" 2>/dev/null)
                if [ -n "$npu_name" ]; then
                    echo -e "  ${MAGENTA}╡${NC} ${WHITE}NPU${NC}          ${MAGENTA}$npu_name${NC}"
                    NPU_DETECTED="$npu_name"
                fi
            fi
        done
    fi
    
    if [ -z "$gpu_vendor" ]; then
        echo -e "  ${YELLOW}╡${NC} ${WHITE}GPU${NC}          ${YELLOW}No GPU detected - using CPU mode${NC}"
        GPU_VENDOR=""
        BACKEND="cpu"
    fi
}

# Component check with visual indicators
check_components() {
    echo -e "${BLUE}┌─ Component Verification ─────────────────────────────────────────────────────────┐${NC}"
    
    # Backend
    echo -ne "${BLUE}│${NC}  ${WHITE}Backend Binary${NC}        "
    if [ -f "./target/release/ghost-link" ] || [ -f "./target/debug/ghost-link" ]; then
        echo -e "${GREEN}✓ Found${NC}  ${DIM}($(ls -1 target/*/ghost-link 2>/dev/null | head -1))${NC}"
        BACKEND_FOUND=1
    else
        echo -e "${YELLOW}⚠ Building...${NC}  ${DIM}(will compile on launch)${NC}"
        BACKEND_FOUND=0
    fi
    
    # GUI
    echo -ne "${BLUE}│${NC}  ${WHITE}React Frontend${NC}        "
    if [ -d "ghostlink_gui_modern" ] && [ -f "ghostlink_gui_modern/package.json" ]; then
        echo -e "${GREEN}✓ Found${NC}  ${DIM}(ghostlink_gui_modern/)${NC}"
        GUI_FOUND=1
    else
        echo -e "${RED}✗ Missing${NC}"
        GUI_FOUND=0
    fi
    
    # Native stack
    echo -ne "${BLUE}│${NC}  ${WHITE}Native Stack Launcher${NC} "
    if [ -f "scripts/run_native_llama_server_stack.sh" ]; then
        echo -e "${GREEN}✓ Found${NC}  ${DIM}(scripts/run_native_llama_server_stack.sh)${NC}"
        NATIVE_FOUND=1
    else
        echo -e "${RED}✗ Missing${NC}"
        NATIVE_FOUND=0
    fi
    
    # Model dir
    echo -ne "${BLUE}│${NC}  ${WHITE}Model Directory${NC}       "
    mkdir -p "$PROJECT_ROOT/models"
    echo -e "${GREEN}✓ Ready${NC}  ${DIM}($PROJECT_ROOT/models/)${NC}"
    
    echo -e "${BLUE}└────────────────────────────────────────────────────────────────────────────────────┘${NC}"
    echo ""
}

# Build llama.cpp if needed
build_llama_cpp() {
    local IS_WINDOWS=0
    if [[ "$(uname -s)" =~ MINGW|MSYS|CYGWIN ]]; then
        IS_WINDOWS=1
    fi
    
    # Check for cmake
    if ! command -v cmake >/dev/null 2>&1; then
        echo -e "${RED}✗${NC} ${BOLD}cmake not found${NC} - required to build llama.cpp"
        echo -e "  Install: ${CYAN}sudo apt-get install cmake${NC} (Linux) or ${CYAN}brew install cmake${NC} (macOS)"
        return 1
    fi
    if ! command -v git >/dev/null 2>&1; then
        echo -e "${RED}✗${NC} ${BOLD}git not found${NC} - required to fetch llama.cpp"
        return 1
    fi
    
    echo -e "  ${WHITE}▶${NC} ${BOLD}Native Inference Stack${NC} ${DIM}(llama.cpp + Ghostlink API)${NC}"
    progress_bar 0 3
    echo " ${DIM}Preparing llama.cpp...${NC}"
    sleep 0.5
    
    # Check if llama.cpp exists and build if needed
    local LLAMA_SERVER_BIN="third_party/llama.cpp/build/bin/llama-server"
    if [ $IS_WINDOWS -eq 1 ]; then
        LLAMA_SERVER_BIN="third_party/llama.cpp/build/bin/Release/llama-server.exe"
        if [ ! -f "$LLAMA_SERVER_BIN" ]; then
            LLAMA_SERVER_BIN="third_party/llama.cpp/build/bin/llama-server.exe"
        fi
    fi
    
    if [ ! -x "$LLAMA_SERVER_BIN" ] && [ ! -f "$LLAMA_SERVER_BIN" ]; then
        progress_bar 1 3
        echo " ${YELLOW}Building llama-server...${NC}      "
        local LLAMA_DIR="third_party/llama.cpp"
        
        if [ ! -d "$LLAMA_DIR" ]; then
            mkdir -p third_party
            git clone https://github.com/ggml-org/llama.cpp.git "$LLAMA_DIR" >/dev/null 2>&1
        fi
        
        cd "$LLAMA_DIR"
        
        # Detect GPU vendor for cmake flags (use detected BACKEND)
        local CMAKE_GPU_FLAGS=""
        if [ "$BACKEND" = "cuda" ]; then
            echo "  Building with CUDA support"
            CMAKE_GPU_FLAGS="-DLLAMA_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES=all"
        elif [ "$BACKEND" = "rocm" ]; then
            echo "  Building with HIP/ROCm support"
            CMAKE_GPU_FLAGS="-DLLAMA_HIPBLAS=ON -DAMDGPU_TARGETS=all"
        elif [ "$BACKEND" = "vulkan" ]; then
            echo "  Building with Vulkan support"
            CMAKE_GPU_FLAGS="-DLLAMA_VULKAN=ON"
        elif [ "$BACKEND" = "metal" ]; then
            echo "  Building with Metal support"
            CMAKE_GPU_FLAGS="-DLLAMA_METAL=ON"
        elif command -v nvidia-smi >/dev/null 2>&1; then
            echo "  Building with CUDA support (nvidia-smi detected)"
            CMAKE_GPU_FLAGS="-DLLAMA_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES=all"
        elif command -v rocm-smi >/dev/null 2>&1; then
            echo "  Building with HIP/ROCm support"
            CMAKE_GPU_FLAGS="-DLLAMA_HIPBLAS=ON -DAMDGPU_TARGETS=all"
        else
            echo "  Building CPU-only llama.cpp"
        fi
        
        if [ -z "$CMAKE_GPU_FLAGS" ]; then
            echo "  No GPU detected — building CPU-only llama.cpp"
        fi
        
        local BUILD_JOBS="${CMAKE_BUILD_PARALLEL_LEVEL:-2}"
        if [ $IS_WINDOWS -eq 1 ]; then
            cmake -S . -B build -DCMAKE_BUILD_TYPE=Release $CMAKE_GPU_FLAGS >/dev/null 2>&1
            cmake --build build --config Release --target llama-server -j "$BUILD_JOBS" >/dev/null 2>&1
        else
            cmake -S . -B build -DCMAKE_BUILD_TYPE=Release $CMAKE_GPU_FLAGS >/dev/null 2>&1
            cmake --build build --config Release --target llama-server -j "$BUILD_JOBS" >/dev/null 2>&1
        fi
        
        local result=$?
        cd - >/dev/null
        
        if [ $result -ne 0 ]; then
            echo -e "\r${CLEAR_LINE}  ${RED}✗${NC} Build failed. Check logs."
            return 1
        fi
    else
        progress_bar 2 3
        echo " ${GREEN}llama.cpp ready${NC}               "
        sleep 0.3
    fi
    
    progress_bar 3 3
    echo " ${GREEN}Native stack ready${NC}            "
    echo ""
}

# Animated service startup
start_services() {
    local API_HOST="${GHOSTLINK_API_HOST:-127.0.0.1}"
    local API_PORT="${GHOSTLINK_API_PORT:-8003}"
    local LLAMA_HOST="${GHOSTLINK_LLAMA_HOST:-127.0.0.1}"
    local LLAMA_PORT="${GHOSTLINK_LLAMA_SERVER_PORT:-8080}"

    # Pre-check for build dependencies
    if ! command -v cmake >/dev/null 2>&1; then
        echo -e "${RED}✗${NC} ${BOLD}cmake not found${NC} - required to build llama.cpp"
        echo -e "  Install: ${CYAN}sudo apt-get install cmake${NC} (Linux) or ${CYAN}brew install cmake${NC} (macOS)"
        return 1
    fi
    if ! command -v git >/dev/null 2>&1; then
        echo -e "${RED}✗${NC} ${BOLD}git not found${NC} - required to fetch llama.cpp"
        return 1
    fi
    if ! command -v cargo >/dev/null 2>&1 || ! command -v rustc >/dev/null 2>&1; then
        echo -e "${RED}✗${NC} ${BOLD}cargo/rustc not found${NC} - required to run Ghostlink API"
        echo -e "  Try: ${CYAN}source \"\$HOME/.cargo/env\"${NC}"
        return 1
    fi
    
    echo -e "${BLUE}┌─ Starting Services ──────────────────────────────────────────────────────────────┐${NC}"
    echo ""
    
    # Detect platform
    local IS_WINDOWS=0
    if [[ "$(uname -s)" =~ MINGW|MSYS|CYGWIN ]]; then
        IS_WINDOWS=1
    fi
    
    # Platform-specific llama-server binary path
    local LLAMA_SERVER_BIN="third_party/llama.cpp/build/bin/llama-server"
    if [ $IS_WINDOWS -eq 1 ]; then
        LLAMA_SERVER_BIN="third_party/llama.cpp/build/bin/Release/llama-server.exe"
        if [ ! -f "$LLAMA_SERVER_BIN" ]; then
            LLAMA_SERVER_BIN="third_party/llama.cpp/build/bin/llama-server.exe"
        fi
    fi
    
    # 1. Native Inference Stack (llama-server)
    echo -e "  ${WHITE}▶${NC} ${BOLD}Native Inference Stack${NC} ${DIM}(llama.cpp + Ghostlink API)${NC}"
    progress_bar 0 3
    echo " ${DIM}Preparing llama.cpp...${NC}"
    sleep 0.5
    
    if [ ! -x "$LLAMA_SERVER_BIN" ] && [ ! -f "$LLAMA_SERVER_BIN" ]; then
        progress_bar 1 3
        echo " ${YELLOW}Building llama-server...${NC}      "
        build_llama_cpp >/tmp/ghostlink-bootstrap.log 2>&1 &
        BOOTSTRAP_PID=$!
        spinner $BOOTSTRAP_PID "Building llama.cpp (this may take 2-5 minutes)..."
        if [ $? -ne 0 ]; then
            echo -e "\r${CLEAR_LINE}  ${RED}✗${NC} Build failed. Check /tmp/ghostlink-bootstrap.log"
            return 1
        fi
    else
        progress_bar 2 3
        echo " ${GREEN}llama.cpp ready${NC}               "
        sleep 0.3
    fi
    
    progress_bar 3 3
    echo " ${GREEN}Native stack ready${NC}            "
    echo ""
    
    # 2. Ghostlink API
    echo -e "  ${WHITE}▶${NC} ${BOLD}Ghostlink API Server${NC} ${DIM}(port ${API_PORT})${NC}"
    progress_bar 0 2
    echo " ${DIM}Starting...${NC}"
    
     # Resource planning based on detected hardware
     GPU_VENDOR="${GPU_VENDOR:-}"
     BACKEND="${BACKEND:-cpu}"
     VRAM_GB="${VRAM_GB:-0}"
     
     if [ -n "${GHOSTLINK_LLAMA_BACKEND:-}" ]; then
         BACKEND="$GHOSTLINK_LLAMA_BACKEND"
     fi
     
     # Plan threads
     local LOGICAL_CORES
     if [[ "$(uname -s)" == "Darwin" ]]; then
         LOGICAL_CORES=$(sysctl -n hw.logicalcpu 2>/dev/null || echo 4)
     else
         LOGICAL_CORES=$(nproc 2>/dev/null || echo 4)
     fi
     if [ -n "${GHOSTLINK_LLAMA_THREADS:-}" ]; then
         THREADS=$GHOSTLINK_LLAMA_THREADS
     else
         THREADS=$(( LOGICAL_CORES > 1 ? LOGICAL_CORES - 1 : 1 ))
     fi
     
# Plan GPU layers
      if [ -n "${GHOSTLINK_LLAMA_NGL:-}" ]; then
          LLAMA_NGL=$GHOSTLINK_LLAMA_NGL
      elif [ "$BACKEND" = "cpu" ]; then
          LLAMA_NGL=0
      else
          LLAMA_NGL=99  # full offload
      fi
     
     # Backend flags
     local BACKEND_FLAGS=""
     case "$BACKEND" in
         cuda)     BACKEND_FLAGS="" ;;
        vulkan)   BACKEND_FLAGS="--vulkan" ;;
         rocm)     BACKEND_FLAGS="" ;;
        metal)    BACKEND_FLAGS="--metal" ;;
        cpu)      BACKEND_FLAGS="" ;;
        *)        BACKEND_FLAGS="" ;;
     esac
     
     # Memory lock
     local MLOCK_FLAG=""
     local TOTAL_RAM_GB
     if [[ "$(uname -s)" == "Darwin" ]]; then
         TOTAL_RAM_GB=$(sysctl -n hw.memsize 2>/dev/null | awk '{printf "%.0f", $1/1073741824}' || echo 4)
     else
         TOTAL_RAM_GB=$(awk '/MemTotal/ {printf "%.0f", $2/1048576}' /proc/meminfo 2>/dev/null || echo 4)
     fi
     if [ "$TOTAL_RAM_GB" -ge 8 ] 2>/dev/null; then
         MLOCK_FLAG="--mlock"
     fi
     
     mkdir -p "$PROJECT_ROOT/models"
     if [ ! -f "$PROJECT_ROOT/models/stories15M-q4_0.gguf" ]; then
         echo -e "  ${DIM}Downloading model...${NC}"
         curl -L --fail -o "$PROJECT_ROOT/models/stories15M-q4_0.gguf" \
             "https://huggingface.co/ggml-org/models/resolve/main/tinyllamas/stories15M-q4_0.gguf" \
             2>/dev/null || echo "  ${YELLOW}⚠ Model download failed, will use existing if any${NC}"
     fi
     
     echo -e "  ${DIM}Starting llama-server: -ngl ${LLAMA_NGL} -t ${THREADS} ${BACKEND_FLAGS}${NC}"
     
      "$LLAMA_SERVER_BIN" \
          -m "$PROJECT_ROOT/models/stories15M-q4_0.gguf" \
          --host "$LLAMA_HOST" --port "$LLAMA_PORT" \
          -ngl "$LLAMA_NGL" \
          -np 1 \
          -t "$THREADS" \
          $MLOCK_FLAG \
          $BACKEND_FLAGS \
          >/tmp/ghostlink_llama_server.log 2>&1 &
    LLAMA_PID=$!
    
    progress_bar 1 2
    echo " ${DIM}Waiting for llama-server...${NC}"
    if ! wait_for_http "http://${LLAMA_HOST}:${LLAMA_PORT}/health" "llama-server" 60; then
        return 1
    fi
    
    # Start Ghostlink API.
    # Use llama-server base URL; native_engine health/model-load code appends endpoint paths.
    GHOSTLINK_INFERENCE_BACKEND=native \
    GHOSTLINK_NATIVE_ENGINE=llama_server \
    GHOSTLINK_LLAMA_SERVER_URL="http://${LLAMA_HOST}:${LLAMA_PORT}" \
    GHOSTLINK_LLAMA_NGL="${LLAMA_NGL:-0}" \
    cargo run -p ghost-link -- serve "$API_HOST" "$API_PORT" \
        >/tmp/ghostlink_api.log 2>&1 &
    API_PID=$!
    
    if ! wait_for_http "http://${API_HOST}:${API_PORT}/health" "Ghostlink API" 60; then
        return 1
    fi
    
    # Wait for API endpoint to be fully ready
    if ! wait_for_http "http://${API_HOST}:${API_PORT}/api/health" "API endpoint" 30; then
        return 1
    fi
    echo ""
    
    # 3. React Frontend
    echo -e "  ${WHITE}▶${NC} ${BOLD}React Frontend (Vite)${NC} ${DIM}(port 5173)${NC}"
    progress_bar 0 3
    echo " ${DIM}Checking dependencies...${NC}"
    
    cd ghostlink_gui_modern
    if [ ! -d "node_modules" ]; then
        progress_bar 1 3
        echo " ${YELLOW}Installing npm packages...${NC}"
        npm install --legacy-peer-deps >/tmp/ghostlink_frontend_install.log 2>&1
    else
        progress_bar 1 3
        echo " ${GREEN}Dependencies cached${NC}         "
    fi
    
    progress_bar 2 3
    echo " ${DIM}Starting Vite dev server...${NC}"
    export VITE_GHOSTLINK_API_BASE="http://${API_HOST}:${API_PORT}"
    npm run dev -- --host 127.0.0.1 --port 5173 >/tmp/ghostlink_frontend.log 2>&1 &
    GUI_PID=$!
    cd ..
    
    if ! wait_for_http "http://127.0.0.1:5173" "React Frontend" 60; then
        return 1
    fi
    progress_bar 3 3
    echo " ${GREEN}Frontend ready${NC}              "
    echo ""
    
    echo -e "${BLUE}└────────────────────────────────────────────────────────────────────────────────────┘${NC}"
    echo ""
}

# Success screen
show_success() {
    echo -e "${GREEN}┌─ Launch Successful ──────────────────────────────────────────────────────────────┐${NC}"
    echo -e "${GREEN}│${NC}                                                                                 ${GREEN}│${NC}"
    echo -e "${GREEN}│${NC}   ${BOLD}${WHITE}███████╗██╗  ██╗██████╗ ███████╗████████╗██╗   ██╗███████╗${NC}                        ${GREEN}│${NC}"
    echo -e "${GREEN}│${NC}   ${BOLD}${WHITE}██╔════╝██║  ██║██╔══██╗██╔════╝╚══██╔══╝██║   ██║██╔════╝${NC}                        ${GREEN}│${NC}"
    echo -e "${GREEN}│${NC}   ${BOLD}${WHITE}█████╗  ███████║██████╔╝█████╗     ██║   ██║   ██║███████╗${NC}                        ${GREEN}│${NC}"
    echo -e "${GREEN}│${NC}   ${BOLD}${WHITE}██╔══╝  ██╔══██║██╔══██╗██╔══╝     ██║   ██║   ██║╚════██║${NC}                        ${GREEN}│${NC}"
    echo -e "${GREEN}│${NC}   ${BOLD}${WHITE}██║     ██║  ██║██║  ██║███████╗   ██║   ╚██████╔╝███████║${NC}                        ${GREEN}│${NC}"
    echo -e "${GREEN}│${NC}   ${BOLD}${WHITE}╚═╝     ╚═╝  ╚═╝╚═╝  ╚═╝╚══════╝   ╚═╝    ╚═════╝ ╚══════╝${NC}                        ${GREEN}│${NC}"
    echo -e "${GREEN}│${NC}                                                                                 ${GREEN}│${NC}"
    echo -e "${GREEN}│${NC}   ${CYAN}Ghostlink Studio is now running!${NC}                                              ${GREEN}│${NC}"
    echo -e "${GREEN}│${NC}                                                                                 ${GREEN}│${NC}"
    echo -e "${GREEN}└────────────────────────────────────────────────────────────────────────────────────┘${NC}"
    echo ""
    
    echo -e "${BLUE}┌─ Service Endpoints ──────────────────────────────────────────────────────────────┐${NC}"
    echo -e "${BLUE}│${NC}  ${WHITE}▶${NC} ${BOLD}Web Interface${NC}      → ${CYAN}http://127.0.0.1:5173${NC}"
    echo -e "${BLUE}│${NC}  ${WHITE}▶${NC} ${BOLD}API Server${NC}         → ${CYAN}http://127.0.0.1:8003${NC}"
    echo -e "${BLUE}│${NC}  ${WHITE}▶${NC} ${BOLD}Native Inference${NC}   → ${CYAN}http://127.0.0.1:8080${NC} ${DIM}(llama-server)${NC}"
    echo -e "${BLUE}│${NC}  ${WHITE}▶${NC} ${BOLD}MCP Gateway${NC}        → ${CYAN}http://127.0.0.1:8811${NC} ${DIM}(100+ MCP servers)${NC}"
    echo -e "${BLUE}└────────────────────────────────────────────────────────────────────────────────────┘${NC}"
    echo ""
    
    echo -e "${YELLOW}┌─ Quick Start ────────────────────────────────────────────────────────────────────┐${NC}"
    echo -e "${YELLOW}│${NC}  ${WHITE}1.${NC} Open ${CYAN}http://127.0.0.1:5173${NC} in your browser"
    echo -e "${YELLOW}│${NC}  ${WHITE}2.${NC} Go to ${BOLD}Models${NC} tab → Select a model"
    echo -e "${YELLOW}│${NC}  ${WHITE}3.${NC} Switch to ${BOLD}Chat${NC} tab → Start talking!"
    echo -e "${YELLOW}│${NC}  ${WHITE}4.${NC} Watch real-time inference with native GPU acceleration"
    echo -e "${YELLOW}│${NC}  ${WHITE}5.${NC} Use ${BOLD}MCP${NC} tab → Access 100+ tool servers (filesystem, git, web, etc.)"
    echo -e "${YELLOW}└────────────────────────────────────────────────────────────────────────────────────┘${NC}"
    echo ""
    
echo -e "  ${DIM}Hardware: ${GPU_NAME:-CPU} (${BACKEND})${NC}"
echo -e "  ${DIM}GPU Layers: ${LLAMA_NGL:-0} | Threads: ${THREADS:-auto}${NC}"
[[ -n "$NPU_DETECTED" ]] && echo -e "  ${DIM}NPU: $NPU_DETECTED${NC}"
echo ""
echo -e "${DIM}GPU Configuration:${NC}"
echo -e "  ${DIM}• OLLAMA_NUM_THREAD: $OLLAMA_NUM_THREAD (all cores)${NC}"
echo -e "  ${DIM}• OLLAMA_GPU_MEMORY: $OLLAMA_GPU_MEMORY (safe 80%%)${NC}"
echo -e "  ${DIM}• HSA_OVERRIDE_GFX_VERSION: $HSA_OVERRIDE_GFX_VERSION${NC}"
echo -e "  ${DIM}• HIP_PLATFORM: $HIP_PLATFORM${NC}"
echo ""
echo -e "${DIM}Press ${BOLD}Ctrl+C${NC} ${DIM}to stop all services${NC}"
echo -e "${DIM}Logs: /tmp/ghostlink_*.log${NC}"
echo ""
}

# Cleanup on exit
cleanup() {
    echo -e "\n${YELLOW}Shutting down services...${NC}"
    [ -n "${GUI_PID:-}" ] && kill $GUI_PID 2>/dev/null && wait $GUI_PID 2>/dev/null
    [ -n "${API_PID:-}" ] && kill $API_PID 2>/dev/null && wait $API_PID 2>/dev/null
    [ -n "${LLAMA_PID:-}" ] && kill $LLAMA_PID 2>/dev/null && wait $LLAMA_PID 2>/dev/null
    echo -e "${GREEN}✓${NC} All services stopped."
    echo -e "${SHOW_CURSOR}"
    tput cnorm 2>/dev/null
    exit 0
}

trap cleanup EXIT INT TERM

# Main cinematic sequence
main() {
    # Detect Windows (Git Bash/WSL) and redirect to launch.bat
    if [[ "$(uname -s)" =~ MINGW|MSYS|CYGWIN ]]; then
        echo -e "${YELLOW}Windows detected.${NC} Please use ${BOLD}launch.bat${NC} instead of launch.sh on Windows."
        echo -e "Run: ${CYAN}.\\launch.bat${NC}"
        exit 1
    fi

    ensure_rust_toolchain || exit 1
    
    show_banner
    sleep 0.5
    
    # Cinematic intro text
    echo -e "${DIM}  Initializing distributed LLM inference fabric...${NC}"
    sleep 0.8
    echo -e "${DIM}  Loading neural pathways...${NC}"
    sleep 0.5
    echo -e "${DIM}  Calibrating tensor cores...${NC}"
    sleep 0.5
    echo ""
    
    show_system_info
    detect_gpu
    check_components
    start_services || { cleanup; exit 1; }
    show_success
    
    # Keep running until Ctrl+C
    wait
}

main "$@"