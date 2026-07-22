#!/usr/bin/env bash

# Ghostlink Studio - GPU-Accelerated Launch (Linux/macOS)
# Optimized for AMD ROCm with automatic GPU environment configuration

# Hardware globals (set by detect_gpu / start_services)
GPU_VENDOR=""
BACKEND="cpu"
VRAM_GB=0
NPU_DETECTED=""
GPU_NAME=""
RAM_GB=0
CPU_CORES=0

export HIP_PLATFORM=${HIP_PLATFORM:-amd}
export HSA_OVERRIDE_GFX_VERSION=${HSA_OVERRIDE_GFX_VERSION:-gfx906}

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

# Ensure Rust toolchain is available even when shell PATH has not sourced ~/.cargo/env.
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

# Cursor will be hidden in main() and shown by cleanup() trap below

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

# Wait for HTTP endpoint with animated spinner.
# Optional 4th arg: a substring that must appear in the response BODY, not
# just a 2xx status. A bare "curl -sf succeeded" is not proof that the
# service we WANT is what answered — any unrelated service already bound to
# the same port (a leftover dev server, a different project's process,
# anything) will also return 200 and fool a status-only check, especially
# since free_port can't do anything about a process that immediately
# respawns (a supervised service, `--reload`, a cron job). Content-checking
# a marker unique to the expected service's response body closes that gap
# regardless of what's supervising the conflicting process or how fast it
# respawns.
# Optional 5th arg: the env var to suggest as a port override in the
# on-timeout diagnostic (e.g. "GHOSTLINK_API_PORT"). Omit for checks where
# no such override applies.
wait_for_http() {
    local url="$1"
    local label="$2"
    local timeout_s="${3:-120}"
    local body_marker="${4:-}"
    local override_var="${5:-}"
    local start
    start=$(date +%s)
    local body

    while true; do
        if [ -n "$body_marker" ]; then
            body=$(curl -sf "$url" 2>/dev/null || true)
            if [ -n "$body" ] && echo "$body" | grep -qF "$body_marker"; then
                printf "\r${CLEAR_LINE}  ${GREEN}✓${NC} ${WHITE}%s${NC} ${GREEN}ready${NC} at ${CYAN}%s${NC}\n" "$label" "$url"
                return 0
            fi
        elif curl -sf "$url" >/dev/null 2>&1; then
            printf "\r${CLEAR_LINE}  ${GREEN}✓${NC} ${WHITE}%s${NC} ${GREEN}ready${NC} at ${CYAN}%s${NC}\n" "$label" "$url"
            return 0
        fi
        if (( $(date +%s) - start >= timeout_s )); then
            printf "\r${CLEAR_LINE}  ${RED}✗${NC} ${WHITE}%s${NC} ${RED}timeout${NC} after %ds: %s\n" "$label" "$timeout_s" "$url"
            if [ -n "$body_marker" ] && [ -n "${body:-}" ]; then
                echo -e "  ${YELLOW}⚠${NC} ${DIM}Something answered ${url} but it isn't ${label} (missing '${body_marker}' in response).${NC}"
                if [ -n "$override_var" ]; then
                    echo -e "  ${DIM}A different service is likely already bound to this port. Pick another with ${override_var}=<port> ./launch.sh${NC}"
                else
                    echo -e "  ${DIM}A different service is likely already bound to this port. Free it, or override the port for this service and retry.${NC}"
                fi
            fi
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
if [[ "$(uname -s)" == "Darwin" ]]; then
    local _ram=$(sysctl -n hw.memsize 2>/dev/null | awk '{printf "%.0f", $1/1073741824}' || echo '?')
    echo -e "${BLUE}│${NC}  ${WHITE}RAM:${NC}          ${_ram} GB"
else
    local _ram=$(awk '/MemTotal/ {printf "%.0f", $2/1048576}' /proc/meminfo 2>/dev/null || echo '?')
    echo -e "${BLUE}│${NC}  ${WHITE}RAM:${NC}          ${_ram} GB"
fi
echo -e "${BLUE}│${NC}  ${WHITE}GPU:${NC}           $(nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null | head -1 || echo 'detecting...')"
if [[ -n "$NPU_DETECTED" ]]; then
    echo -e "${BLUE}│${NC}  ${WHITE}NPU:${NC}           $NPU_DETECTED"
fi
echo -e "${BLUE}└────────────────────────────────────────────────────────────────────────────────────┘${NC}"
echo ""
}

# CPU & RAM detection
detect_cpu_ram() {
    if [[ "$(uname -s)" == "Darwin" ]]; then
        CPU_CORES=$(sysctl -n hw.logicalcpu 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)
        RAM_GB=$(sysctl -n hw.memsize 2>/dev/null | awk '{printf "%.0f", $1/1073741824}' || echo 4)
    else
        CPU_CORES=$(nproc 2>/dev/null || grep -c ^processor /proc/cpuinfo 2>/dev/null || echo 4)
        RAM_GB=$(awk '/MemTotal/ {printf "%.0f", $2/1048576}' /proc/meminfo 2>/dev/null || echo 4)
    fi
    echo -e "  ${GREEN}╡${NC} ${WHITE}CPU${NC}          ${CPU_CORES} cores"
    echo -e "  ${GREEN}╡${NC} ${WHITE}RAM${NC}          ${RAM_GB} GB"
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
            GPU_NAME="$gpu_name"
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
            GPU_NAME="$gpu_name"
        else
            echo -e "  ${GREEN}╡${NC} ${WHITE}GPU${NC}          ${GREEN}AMD ROCm detected (gfx906 mapping)${NC}"
            GPU_NAME="AMD ROCm"
        fi
        gpu_vendor="amd"
        GPU_VENDOR="amd"
        BACKEND="rocm"
    fi
    
    # AMD/Intel via lspci (Linux)
    if [ -z "$gpu_vendor" ] && command -v lspci >/dev/null 2>&1; then
        local gpu_line
        gpu_line=$(lspci -mm 2>/dev/null | grep -iE "(VGA compatible|3D controller)" | head -1)
        if [ -n "$gpu_line" ]; then
            gpu_name=$(echo "$gpu_line" | grep -iEo '"([^"]*)"' | tail -1 | tr -d '"')
            if echo "$gpu_name" | grep -qiE "amd|radeon|advanced micro"; then
                echo -e "  ${GREEN}╡${NC} ${WHITE}GPU${NC}          ${GREEN}AMD: ${gpu_name:-AMD GPU}${NC}"
                GPU_NAME="${gpu_name:-AMD GPU}"
                gpu_vendor="amd"
                GPU_VENDOR="amd"
                BACKEND="rocm"
            elif echo "$gpu_name" | grep -qiE "intel|arc|iris|uhd"; then
                echo -e "  ${GREEN}╡${NC} ${WHITE}GPU${NC}          ${GREEN}Intel: ${gpu_name}${NC}"
                GPU_NAME="$gpu_name"
                gpu_vendor="intel"
                GPU_VENDOR="intel"
                BACKEND="vulkan"
            else
                echo -e "  ${GREEN}╡${NC} ${WHITE}GPU${NC}          ${GREEN}GPU: ${gpu_name}${NC}"
                GPU_NAME="$gpu_name"
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
            GPU_NAME="$gpu_name"
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
        GPU_NAME="CPU"
        GPU_VENDOR=""
        BACKEND="cpu"
    fi
}

# Component check with visual indicators
check_components() {
    echo -e "${BLUE}┌─ Component Verification ─────────────────────────────────────────────────────────┐${NC}"
    
    # Backend (native ELF only — ignore Windows ghost-link.exe under WSL)
    echo -ne "${BLUE}│${NC}  ${WHITE}Backend Binary${NC}        "
    local _api_bin=""
    if _api_bin=$(resolve_api_bin 2>/dev/null); then
        echo -e "${GREEN}✓ Found${NC}  ${DIM}(${_api_bin})${NC}"
        BACKEND_FOUND=1
    else
        echo -e "${YELLOW}⚠ Building...${NC}  ${DIM}(Linux binary required under WSL)${NC}"
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
            git clone https://github.com/ggml-org/llama.cpp.git "$LLAMA_DIR" >/dev/null 2>&1 || return 1
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

# True when running under WSL (Linux kernel, Windows host paths)
is_wsl() {
    if [ -n "${WSL_DISTRO_NAME:-}" ] || [ -n "${WSL_INTEROP:-}" ]; then
        return 0
    fi
    grep -qiE 'microsoft|wsl' /proc/version 2>/dev/null
}

# Host OS family for binary selection (never run Windows PE under WSL/Linux)
host_os_family() {
    local u
    u=$(uname -s 2>/dev/null || echo unknown)
    if [[ "$u" =~ MINGW|MSYS|CYGWIN ]]; then
        echo windows
    elif is_wsl; then
        echo linux
    else
        case "$u" in
            Linux*) echo linux ;;
            Darwin*) echo darwin ;;
            *) echo other ;;
        esac
    fi
}

# Reject Windows PE binaries on Linux/WSL (they "run" via interop but break on /mnt paths)
is_runnable_native_bin() {
    local path="$1"
    [ -n "$path" ] && [ -f "$path" ] || return 1
    local family
    family=$(host_os_family)
    case "$path" in
        *.exe|*.EXE|*.dll|*.DLL)
            [ "$family" = "windows" ] || return 1
            ;;
    esac
    if [ "$family" != "windows" ] && command -v file >/dev/null 2>&1; then
        local ft
        ft=$(file -b "$path" 2>/dev/null || true)
        if echo "$ft" | grep -qiE 'PE32|MS Windows|MS-DOS'; then
            return 1
        fi
    fi
    return 0
}

# Prefer a real Linux/macOS node; ignore Windows node.exe on PATH via WSL interop
resolve_node_bin() {
    local candidate
    # Load nvm if present (common under WSL)
    if [ -z "${NVM_DIR:-}" ] && [ -s "$HOME/.nvm/nvm.sh" ]; then
        export NVM_DIR="$HOME/.nvm"
        # shellcheck disable=SC1091
        . "$NVM_DIR/nvm.sh" >/dev/null 2>&1 || true
    elif [ -n "${NVM_DIR:-}" ] && [ -s "$NVM_DIR/nvm.sh" ]; then
        # shellcheck disable=SC1091
        . "$NVM_DIR/nvm.sh" >/dev/null 2>&1 || true
    fi
    # Newest nvm node first
    local nvm_nodes=()
    if compgen -G "$HOME/.nvm/versions/node/*/bin/node" >/dev/null 2>&1; then
        # shellcheck disable=SC2207
        nvm_nodes=($(ls -1d "$HOME/.nvm/versions/node/"*/bin/node 2>/dev/null | sort -V -r))
    fi
    for candidate in \
        "${GHOSTLINK_NODE_BIN:-}" \
        "${nvm_nodes[@]}" \
        "$HOME/.local/share/fnm/node-versions/"*/installation/bin/node \
        /usr/local/bin/node \
        /usr/bin/node
    do
        if is_runnable_native_bin "$candidate" && [ -x "$candidate" ]; then
            echo "$candidate"
            return 0
        fi
    done
    if command -v node >/dev/null 2>&1; then
        candidate=$(command -v node)
        # Reject Windows node.exe exposed through WSL interop
        case "$candidate" in
            /mnt/c/*|*/Program\ Files/*|*.exe) return 1 ;;
        esac
        if is_runnable_native_bin "$candidate"; then
            echo "$candidate"
            return 0
        fi
    fi
    return 1
}

# Free a TCP port (best-effort) so stale processes do not cause bind errors / 405 from wrong servers
# Tool-independent fallback: find PIDs with a LISTEN socket on $1 by walking
# /proc directly. Needed on minimal hosts without fuser/lsof/ss/netstat —
# without this, free_port() silently no-ops on such hosts, a stale listener
# is never actually killed, and a later health check can be fooled into
# reporting a freshly-started replacement as "ready" when it's really still
# talking to the old process (which then fails on any route added since).
proc_net_pids_for_port() {
    local port="$1"
    [ -r /proc/net/tcp ] || return 0
    local hex_port
    hex_port=$(printf '%04X' "$port")
    local inodes
    inodes=$( { cat /proc/net/tcp 2>/dev/null; cat /proc/net/tcp6 2>/dev/null; } \
        | awk -v hp="$hex_port" 'NR>1 { split($2,a,":"); if (a[2]==hp && $4=="0A") print $10 }' \
        | sort -u)
    [ -n "$inodes" ] || return 0
    local inode fd link pid
    local -A seen=()
    for fd in /proc/[0-9]*/fd/*; do
        link=$(readlink "$fd" 2>/dev/null) || continue
        case "$link" in
            socket:\[*\])
                for inode in $inodes; do
                    if [ "$link" = "socket:[$inode]" ]; then
                        pid="${fd#/proc/}"
                        pid="${pid%%/*}"
                        if [ -z "${seen[$pid]:-}" ]; then
                            seen[$pid]=1
                            echo "$pid"
                        fi
                    fi
                done
                ;;
        esac
    done
}

free_port() {
    local port="$1"
    local pids=""
    if command -v fuser >/dev/null 2>&1; then
        fuser -k "${port}/tcp" >/dev/null 2>&1 || true
    elif command -v lsof >/dev/null 2>&1; then
        pids=$(lsof -ti "tcp:${port}" -sTCP:LISTEN 2>/dev/null || lsof -ti ":${port}" 2>/dev/null || true)
    elif command -v ss >/dev/null 2>&1; then
        pids=$(ss -lptn "sport = :${port}" 2>/dev/null | grep -oE 'pid=[0-9]+' | cut -d= -f2 | sort -u || true)
    elif command -v netstat >/dev/null 2>&1 && [[ "$(uname -s)" != "Darwin" ]]; then
        pids=$(netstat -tlnp 2>/dev/null | awk -v p=":${port}" '$4 ~ p"$" {print $7}' | cut -d/ -f1 | grep -E '^[0-9]+$' || true)
    fi
    # Always cross-check via /proc directly — covers hosts with none of the
    # above tools, and catches orphaned processes the tool-based lookup missed.
    if [[ "$(uname -s)" != "Darwin" ]]; then
        pids="$pids $(proc_net_pids_for_port "$port" 2>/dev/null || true)"
    fi
    if [ -n "${pids// /}" ]; then
        # shellcheck disable=SC2086
        kill -9 $pids >/dev/null 2>&1 || true
    fi
}

# Resolve llama-server binary (absolute path). Never returns Windows .exe under WSL/Linux.
resolve_llama_server_bin() {
    local candidate
    local family
    family=$(host_os_family)
    local candidates=(
        "${GHOSTLINK_LLAMA_SERVER_BIN:-}"
        "${GHOSTLINK_WSL_BIN_DIR:-$HOME/.cache/ghostlink/bin}/llama-server"
        "$PROJECT_ROOT/bin/llama-server"
        "$PROJECT_ROOT/third_party/llama.cpp/build/bin/llama-server"
        "$PROJECT_ROOT/target/release/llama-server"
        "$PROJECT_ROOT/target/debug/llama-server"
    )
    if [ "$family" = "windows" ]; then
        candidates+=(
            "$PROJECT_ROOT/third_party/llama.cpp/build/bin/Release/llama-server.exe"
            "$PROJECT_ROOT/third_party/llama.cpp/build/bin/llama-server.exe"
        )
    fi
    for candidate in "${candidates[@]}"; do
        if is_runnable_native_bin "$candidate"; then
            echo "$candidate"
            return 0
        fi
    done
    if command -v llama-server >/dev/null 2>&1; then
        candidate=$(command -v llama-server)
        if is_runnable_native_bin "$candidate"; then
            echo "$candidate"
            return 0
        fi
    fi
    return 1
}

# Resolve ghost-link API binary (Linux ELF under WSL — never ghost-link.exe)
resolve_api_bin() {
    local candidate
    for candidate in \
        "${GHOSTLINK_API_BIN:-}" \
        "${CARGO_TARGET_DIR:-}/release/ghost-link" \
        "${CARGO_TARGET_DIR:-}/debug/ghost-link" \
        "$HOME/.cache/ghostlink/target/release/ghost-link" \
        "$HOME/.cache/ghostlink/target/debug/ghost-link" \
        "$PROJECT_ROOT/target/release/ghost-link" \
        "$PROJECT_ROOT/target/debug/ghost-link" \
        "$PROJECT_ROOT/bin/ghost-link"
    do
        if is_runnable_native_bin "$candidate" && [ -x "$candidate" ]; then
            echo "$candidate"
            return 0
        fi
    done
    return 1
}

# Ensure a native Linux/macOS ghost-link exists (build if only Windows .exe is present)
ensure_api_bin() {
    local bin
    if bin=$(resolve_api_bin); then
        echo "$bin"
        return 0
    fi
    if ! command -v cargo >/dev/null 2>&1; then
        echo -e "  ${RED}✗${NC} ghost-link not found and cargo is not on PATH" >&2
        if is_wsl; then
            echo -e "  ${DIM}WSL needs a Linux build. Install Rust: https://rustup.rs${NC}" >&2
            echo -e "  ${DIM}Or launch from Windows instead: .\\launch.bat${NC}" >&2
        fi
        return 1
    fi
    # On WSL, building into /mnt/c is extremely slow and can mix with Windows artifacts.
    # Use a Linux-native target dir under $HOME unless the user overrides it.
    if is_wsl && [ -z "${CARGO_TARGET_DIR:-}" ]; then
        export CARGO_TARGET_DIR="${GHOSTLINK_CARGO_TARGET_DIR:-$HOME/.cache/ghostlink/target}"
        mkdir -p "$CARGO_TARGET_DIR"
        echo -e "  ${DIM}CARGO_TARGET_DIR=${CARGO_TARGET_DIR}${NC}" >&2
    fi
    echo -e "  ${YELLOW}Building Linux ghost-link (Windows .exe cannot serve under WSL)...${NC}" >&2
    (
        cd "$PROJECT_ROOT" || exit 1
        cargo build --release -p ghost-link
    ) >/tmp/ghostlink_api_build.log 2>&1
    # resolve after build (also check CARGO_TARGET_DIR)
    if bin=$(resolve_api_bin); then
        echo "$bin"
        return 0
    fi
    for candidate in \
        "${CARGO_TARGET_DIR:-}/release/ghost-link" \
        "$HOME/.cache/ghostlink/target/release/ghost-link"
    do
        if is_runnable_native_bin "$candidate" && [ -x "$candidate" ]; then
            echo "$candidate"
            return 0
        fi
    done
    echo -e "  ${RED}✗${NC} cargo build failed — see /tmp/ghostlink_api_build.log" >&2
    tail -30 /tmp/ghostlink_api_build.log >&2 2>/dev/null || true
    return 1
}

# Pick best available GGUF model (prefer production models used on Windows, fall back to tiny)
resolve_model_file() {
    local candidate
    if [ -n "${GHOSTLINK_MODEL_FILE:-}" ] && [ -f "$GHOSTLINK_MODEL_FILE" ]; then
        echo "$GHOSTLINK_MODEL_FILE"
        return 0
    fi
    for candidate in \
        "$PROJECT_ROOT/models/Llama-3.2-1B-Instruct-IQ3_M.gguf" \
        "$PROJECT_ROOT/models/gemma-4-E4B-it-Q4_K_M.gguf" \
        "$PROJECT_ROOT/models/Llama-3.2-3B-Instruct-IQ3_M.gguf" \
        "$PROJECT_ROOT/models/tinyllama-1.1b-chat-v1.0.Q2_K.gguf" \
        "$PROJECT_ROOT/models/stories15M-q4_0.gguf"
    do
        if [ -f "$candidate" ]; then
            echo "$candidate"
            return 0
        fi
    done
    return 1
}

# Official prebuilt llama.cpp (Linux x86_64) when local build is unavailable
download_prebuilt_llama() {
    if [ "$(uname -s)" != "Linux" ] || [ "$(uname -m)" != "x86_64" ]; then
        return 1
    fi
    # Prefer Linux-native dir under WSL home (fast); fall back to repo bin/
    local dest="$PROJECT_ROOT/bin"
    if is_wsl; then
        dest="${GHOSTLINK_WSL_BIN_DIR:-$HOME/.cache/ghostlink/bin}"
    fi
    # Thin launcher needs sibling .so bundle (keep symlinks intact)
    if [ -x "$dest/llama-server" ] && is_runnable_native_bin "$dest/llama-server" \
        && { [ -e "$dest/libllama-server-impl.so" ] || [ -e "$dest/libllama-common.so.0" ]; } \
        && env LD_LIBRARY_PATH="$dest" "$dest/llama-server" --version >/dev/null 2>&1; then
        echo "$dest/llama-server"
        return 0
    fi
    local variant="ubuntu-x64"
    if ldconfig -p 2>/dev/null | grep -q "libvulkan.so.1"; then
        variant="ubuntu-vulkan-x64"
    fi
    local url
    url=$(curl -fsSL "https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=15" 2>/dev/null \
        | grep -oE "https://github.com/ggml-org/llama.cpp/releases/download/[^\"]+-bin-${variant}\\.tar\\.gz" \
        | head -1)
    [ -n "$url" ] || return 1
    local tmp_tar tmp_dir
    tmp_tar=$(mktemp) || return 1
    tmp_dir=$(mktemp -d) || { rm -f "$tmp_tar"; return 1; }
    mkdir -p "$dest"
    echo -e "  ${DIM}Downloading llama-server (${variant})...${NC}" >&2
    if ! curl -fL --retry 3 --progress-bar -o "$tmp_tar" "$url"; then
        rm -f "$tmp_tar"
        rm -rf "$tmp_dir"
        return 1
    fi
    if ! tar xzf "$tmp_tar" -C "$tmp_dir"; then
        rm -f "$tmp_tar"
        rm -rf "$tmp_dir"
        return 1
    fi
    rm -f "$tmp_tar"
    local found src_dir
    found=$(find "$tmp_dir" -type f -name 'llama-server' | head -1)
    if [ -z "$found" ] || [ ! -f "$found" ]; then
        rm -rf "$tmp_dir"
        return 1
    fi
    src_dir=$(dirname "$found")
    # Copy full runtime bundle (thin llama-server + all .so deps, keep symlinks)
    mkdir -p "$dest"
    cp -a "$found" "$dest/llama-server"
    # Include regular files and symlinks (*.so / *.so.*)
    find "$src_dir" -maxdepth 1 \( -type f -o -type l \) \( -name '*.so' -o -name '*.so.*' \) \
        -exec cp -a {} "$dest/" \;
    rm -rf "$tmp_dir"
    chmod +x "$dest/llama-server" 2>/dev/null || true
    if ! is_runnable_native_bin "$dest/llama-server"; then
        return 1
    fi
    if [ ! -e "$dest/libllama-server-impl.so" ] && [ ! -e "$dest/libllama-common.so.0" ]; then
        return 1
    fi
    # Smoke-check dynamic linker can resolve deps when LD_LIBRARY_PATH=dest
    if ! env LD_LIBRARY_PATH="$dest" "$dest/llama-server" --version >/dev/null 2>&1; then
        echo -e "  ${YELLOW}⚠${NC} llama-server failed --version smoke check" >&2
        env LD_LIBRARY_PATH="$dest" ldd "$dest/llama-server" 2>&1 | grep -i "not found" >&2 || true
        return 1
    fi
    echo "$dest/llama-server"
}

# Animated service startup
start_services() {
    cd "$PROJECT_ROOT" || return 1

    ensure_rust_toolchain || return 1

    local BACKEND_HOST="${GHOSTLINK_API_HOST:-127.0.0.1}"
    local BACKEND_PORT="${GHOSTLINK_API_PORT:-8003}"
    local GUI_PORT="${GUI_PORT:-5173}"
    local LLAMA_PORT="${GHOSTLINK_LLAMA_SERVER_PORT:-8080}"
    # native (default, matches Windows launch-ollama.bat) or ollama
    local INFERENCE_BACKEND="${GHOSTLINK_INFERENCE_BACKEND:-native}"
    INFERENCE_BACKEND=$(echo "$INFERENCE_BACKEND" | tr '[:upper:]' '[:lower:]')

    echo -e "${BLUE}┌─ Starting Services ──────────────────────────────────────────────────────────────┐${NC}"
    echo ""
    echo -e "  ${DIM}Inference backend: ${INFERENCE_BACKEND}${NC}"

    # Clear stale listeners that cause 404/405 and bind failures.
    # Scoped to the exact ports we're about to bind (via free_port's PID lookup on
    # that port) rather than a blind `pkill -f llama-server` / `pkill -f ghost-link`,
    # which would kill any matching process system-wide — including one started by
    # a different user, session, or repo that just happens to share the binary name.
    free_port "$BACKEND_PORT"
    free_port "$GUI_PORT"
    if [ "$INFERENCE_BACKEND" = "native" ]; then
        free_port "$LLAMA_PORT"
    fi
    sleep 0.5

    GPU_VENDOR="${GPU_VENDOR:-}"
    BACKEND="${BACKEND:-cpu}"
    VRAM_GB="${VRAM_GB:-0}"
    if [ -n "${GHOSTLINK_LLAMA_BACKEND:-}" ]; then
        BACKEND="$GHOSTLINK_LLAMA_BACKEND"
    fi

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

    if [ -n "${GHOSTLINK_LLAMA_NGL:-}" ]; then
        LLAMA_NGL=$GHOSTLINK_LLAMA_NGL
    elif [ "$BACKEND" = "cpu" ]; then
        LLAMA_NGL=0
    elif [ "$VRAM_GB" -ge 12 ] 2>/dev/null; then
        LLAMA_NGL=40
    elif [ "$VRAM_GB" -ge 8 ] 2>/dev/null; then
        LLAMA_NGL=24
    elif [ "$VRAM_GB" -ge 4 ] 2>/dev/null; then
        LLAMA_NGL=99
    else
        LLAMA_NGL=99
    fi

    if [ "$BACKEND" = "rocm" ]; then
        export HIP_PLATFORM="${HIP_PLATFORM:-amd}"
        export HSA_OVERRIDE_GFX_VERSION="${HSA_OVERRIDE_GFX_VERSION:-gfx906}"
    fi

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
    export GHOSTLINK_VRAM_GB="${VRAM_GB:-0}"
    export GHOSTLINK_LLAMA_NGL="${LLAMA_NGL:-0}"
    export GHOSTLINK_LLAMA_THREADS="${THREADS}"
    # Only pass the detected GPU name through when a real vendor was found — the
    # Rust-side auto-discovery treats GHOSTLINK_GPU_NAME as an explicit override,
    # so this must stay unset in CPU-only mode (GPU_VENDOR empty) to let it fall
    # through to real hardware probes instead of reporting a fake GPU.
    if [ -n "${GPU_VENDOR:-}" ]; then
        export GHOSTLINK_GPU_NAME="${GPU_NAME}"
    fi

    local NATIVE_ENGINE="simulated"
    LLAMA_PID=""
    MODEL_FILE=""
    MODEL_ALIAS=""
    LLAMA_SERVER_BIN=""

    if [ "$INFERENCE_BACKEND" = "ollama" ]; then
        echo -e "  ${WHITE}▶${NC} ${BOLD}Ollama Inference${NC} ${DIM}(external runtime)${NC}"
        if ! command -v ollama >/dev/null 2>&1; then
            echo -e "  ${RED}✗${NC} ollama not found on PATH"
            echo -e "  ${DIM}Install Ollama or set GHOSTLINK_INFERENCE_BACKEND=native${NC}"
            return 1
        fi
        if ! curl -sf "http://127.0.0.1:11434/api/tags" >/dev/null 2>&1; then
            echo -e "  ${DIM}Starting ollama serve...${NC}"
            ollama serve >/tmp/ghostlink_ollama.log 2>&1 &
            sleep 2
        fi
        if ! wait_for_http "http://127.0.0.1:11434/" "Ollama" 30; then
            echo -e "  ${YELLOW}⚠${NC} Ollama health check failed — API will still start"
        else
            echo -e "  ${GREEN}✓${NC} Ollama ready"
        fi
        NATIVE_ENGINE="ollama"
        echo ""
    else
        # 1. Native Inference Stack (llama-server) — matches Windows launch-ollama.bat
        INFERENCE_BACKEND="native"
        echo -e "  ${WHITE}▶${NC} ${BOLD}Native Inference Stack${NC} ${DIM}(llama.cpp + Ghostlink API)${NC}"
        progress_bar 0 3
        echo " ${DIM}Preparing llama.cpp...${NC}"

        if LLAMA_SERVER_BIN=$(resolve_llama_server_bin); then
            progress_bar 2 3
            echo " ${GREEN}llama.cpp ready${NC}               "
        else
            progress_bar 1 3
            echo " ${YELLOW}Building / downloading Linux llama-server...${NC}"
            # Prefer official Linux prebuilt under WSL (Windows .exe is unusable here)
            if is_wsl || [ "$(host_os_family)" = "linux" ]; then
                LLAMA_SERVER_BIN=$(download_prebuilt_llama 2>/tmp/ghostlink-prebuilt.log) || true
            fi
            if [ -z "${LLAMA_SERVER_BIN:-}" ] || ! is_runnable_native_bin "${LLAMA_SERVER_BIN:-}"; then
                if command -v cmake >/dev/null 2>&1; then
                    build_llama_cpp >/tmp/ghostlink-bootstrap.log 2>&1 || true
                fi
                LLAMA_SERVER_BIN=$(resolve_llama_server_bin || true)
            fi
            if [ -z "${LLAMA_SERVER_BIN:-}" ] || ! is_runnable_native_bin "${LLAMA_SERVER_BIN:-}"; then
                LLAMA_SERVER_BIN=$(download_prebuilt_llama 2>/tmp/ghostlink-prebuilt.log) || true
            fi
            if [ -z "${LLAMA_SERVER_BIN:-}" ] || ! is_runnable_native_bin "${LLAMA_SERVER_BIN:-}"; then
                echo -e "\r${CLEAR_LINE}  ${RED}✗${NC} Native llama-server not found for $(host_os_family)"
                echo -e "  ${DIM}Windows .exe cannot load models from WSL paths (/mnt/c/...).${NC}"
                echo -e "  ${DIM}Fix: set GHOSTLINK_LLAMA_SERVER_BIN to a Linux llama-server, or run .\\launch.bat on Windows.${NC}"
                return 1
            fi
        fi
        if ! is_runnable_native_bin "$LLAMA_SERVER_BIN"; then
            echo -e "  ${RED}✗${NC} Refusing non-native llama-server: $LLAMA_SERVER_BIN"
            echo -e "  ${DIM}Under WSL use a Linux build (prebuilt download or cmake), not llama-server.exe${NC}"
            return 1
        fi
        chmod +x "$LLAMA_SERVER_BIN" 2>/dev/null || true
        export GHOSTLINK_LLAMA_SERVER_BIN="$LLAMA_SERVER_BIN"
        # Prebuilt tarballs ship .so next to the binary
        export LD_LIBRARY_PATH="$(dirname "$LLAMA_SERVER_BIN"):${LD_LIBRARY_PATH:-}"

        if ! MODEL_FILE=$(resolve_model_file); then
            echo -e "  ${DIM}Downloading default tiny model...${NC}"
            curl -L --fail -o "$PROJECT_ROOT/models/stories15M-q4_0.gguf" \
                "https://huggingface.co/ggml-org/models/resolve/main/tinyllamas/stories15M-q4_0.gguf" \
                2>/dev/null || true
            MODEL_FILE=$(resolve_model_file || true)
        fi
        if [ -z "${MODEL_FILE:-}" ] || [ ! -f "$MODEL_FILE" ]; then
            echo -e "  ${RED}✗${NC} No GGUF model found in $PROJECT_ROOT/models"
            return 1
        fi
        MODEL_ALIAS=$(basename "$MODEL_FILE" .gguf)

        progress_bar 3 3
        echo " ${GREEN}Native stack ready${NC}            "
        echo ""
        # Context size: never leave model default (often 128k) on consumer GPUs.
        if [ -n "${GHOSTLINK_CTX_SIZE:-}" ]; then
            CTX_SIZE=$GHOSTLINK_CTX_SIZE
        elif [ "${VRAM_GB:-0}" -ge 16 ] 2>/dev/null; then
            CTX_SIZE=16384
        elif [ "${VRAM_GB:-0}" -ge 12 ] 2>/dev/null; then
            CTX_SIZE=8192
        elif [ "${VRAM_GB:-0}" -ge 8 ] 2>/dev/null; then
            CTX_SIZE=4096
        else
            CTX_SIZE=2048
        fi
        export GHOSTLINK_CTX_SIZE="$CTX_SIZE"

        # Perf defaults: Flash Attention + VRAM-scaled batch + compact KV
        if [ -n "${GHOSTLINK_LLAMA_SERVER_ARGS:-}" ]; then
            LLAMA_PERF_ARGS=($GHOSTLINK_LLAMA_SERVER_ARGS)
        elif [ "$BACKEND" = "cpu" ]; then
            # Keep CPU defaults broadly compatible across tiny and legacy GGUF models.
            LLAMA_PERF_ARGS=(-fa on -b 512 -ub 128)
        elif [ "${VRAM_GB:-0}" -ge 12 ] 2>/dev/null; then
            LLAMA_PERF_ARGS=(-fa on -b 2048 -ub 512 -ctk q8_0 -ctv q8_0)
        elif [ "${VRAM_GB:-0}" -ge 8 ] 2>/dev/null; then
            LLAMA_PERF_ARGS=(-fa on -b 1024 -ub 512 -ctk q8_0 -ctv q8_0)
        elif [ "${VRAM_GB:-0}" -ge 4 ] 2>/dev/null; then
            LLAMA_PERF_ARGS=(-fa on -b 512 -ub 256 -ctk q8_0 -ctv q8_0)
        else
            LLAMA_PERF_ARGS=(-fa on -b 512 -ub 128 -ctk q8_0 -ctv q8_0)
        fi

        if [ -z "${GHOSTLINK_LLAMA_SERVER_ARGS:-}" ] \
            && [[ "$MODEL_ALIAS" =~ ^(stories15M-q4_0|tinyllama-1\.1b-chat-v1\.0\.Q2_K)$ ]]; then
            LLAMA_PERF_ARGS=(-fa on -b 512 -ub 128)
        fi
        export GHOSTLINK_LLAMA_SERVER_ARGS="${GHOSTLINK_LLAMA_SERVER_ARGS:-${LLAMA_PERF_ARGS[*]}}"

        # mlock only when RAM is plentiful (avoids thrash on 16–32GB hosts under load)
        if [ "${GHOSTLINK_MLOCK:-}" = "1" ]; then
            MLOCK_FLAG="--mlock"
        elif [ "${GHOSTLINK_MLOCK:-}" = "0" ]; then
            MLOCK_FLAG=""
        elif [ "$TOTAL_RAM_GB" -lt 24 ] 2>/dev/null; then
            MLOCK_FLAG=""
        fi

        echo -e "  ${DIM}Model: ${MODEL_ALIAS}${NC}"
        echo -e "  ${DIM}llama-server: ${LLAMA_SERVER_BIN}${NC}"
        echo -e "  ${DIM}Starting llama-server: -c ${CTX_SIZE} -ngl ${LLAMA_NGL} -t ${THREADS} ${LLAMA_PERF_ARGS[*]}${NC}"

        # Resolve to a real path (helps when models live on /mnt/c under WSL)
        MODEL_FILE="$(readlink -f "$MODEL_FILE" 2>/dev/null || echo "$MODEL_FILE")"
        if [ ! -r "$MODEL_FILE" ]; then
            echo -e "  ${RED}✗${NC} Model not readable: $MODEL_FILE"
            return 1
        fi

        env LD_LIBRARY_PATH="$(dirname "$LLAMA_SERVER_BIN"):${LD_LIBRARY_PATH:-}" \
        "$LLAMA_SERVER_BIN" \
            -m "$MODEL_FILE" \
            --alias "$MODEL_ALIAS" \
            --host 127.0.0.1 --port "$LLAMA_PORT" \
            -c "$CTX_SIZE" \
            -ngl "$LLAMA_NGL" \
            -np 1 \
            -t "$THREADS" \
            "${LLAMA_PERF_ARGS[@]}" \
            $MLOCK_FLAG \
            >/tmp/ghostlink_llama_server.log 2>&1 &
        LLAMA_PID=$!

        # llama-server's own /health body is just {"status":"ok"} — too generic
        # to prove it's actually llama.cpp (the exact same false-positive class
        # fixed for the Ghostlink API's own health checks applies here too, e.g.
        # any web app already bound to this port that happens to have its own
        # /health route). /v1/models includes "owned_by":"llamacpp", which is
        # distinctive enough that nothing else plausibly returns it.
        if ! wait_for_http "http://127.0.0.1:${LLAMA_PORT}/v1/models" "llama-server" 90 "llamacpp" "GHOSTLINK_LLAMA_SERVER_PORT"; then
            echo -e "  ${RED}✗${NC} llama-server failed — see /tmp/ghostlink_llama_server.log"
            echo -e "  ${DIM}Tip: ensure port ${LLAMA_PORT} is free (another web app there will fool a bare health check).${NC}"
            return 1
        fi
        NATIVE_ENGINE="llama_server"
        echo ""
    fi

    # 2. Ghostlink API — ONLY valid GUI API surface (port 8003 by default).
    #    Do NOT point the GUI at llama-server (:8080) or a bare control-plane without /api proxy.
    #    Wrong port => 404/405 on chat, models, settings.
    if [ "$BACKEND_PORT" = "8080" ] || [ "$BACKEND_PORT" = "11434" ]; then
        echo -e "  ${RED}✗${NC} GHOSTLINK_API_PORT=${BACKEND_PORT} is an inference port, not the Ghostlink API."
        echo -e "  ${DIM}Use GHOSTLINK_API_PORT=8003 (default).${NC}"
        return 1
    fi

    echo -e "  ${WHITE}▶${NC} ${BOLD}Ghostlink API Server${NC} ${DIM}(port ${BACKEND_PORT})${NC}"
    progress_bar 0 2
    echo " ${DIM}Starting...${NC}"

    export GHOSTLINK_INFERENCE_BACKEND="$INFERENCE_BACKEND"
    export GHOSTLINK_NATIVE_ENGINE="$NATIVE_ENGINE"
    if [ "$INFERENCE_BACKEND" = "native" ]; then
        export GHOSTLINK_LLAMA_SERVER_URL="http://127.0.0.1:${LLAMA_PORT}"
    fi
    export GHOSTLINK_LLAMA_NGL="${LLAMA_NGL:-0}"
    export GHOSTLINK_LLAMA_THREADS="${THREADS}"
    # Always pin GUI → ghost-link API (never llama-server / ollama ports)
    export VITE_GHOSTLINK_API_BASE="http://${BACKEND_HOST}:${BACKEND_PORT}"
    export VITE_PROXY_TARGET="http://${BACKEND_HOST}:${BACKEND_PORT}"
    if [ -n "${MODEL_FILE:-}" ]; then
        export GHOSTLINK_MODEL_PATH="$MODEL_FILE"
    fi
    if [ -n "${LLAMA_SERVER_BIN:-}" ]; then
        export GHOSTLINK_LLAMA_SERVER_BIN="$LLAMA_SERVER_BIN"
    fi

    local API_BIN=""
    local API_LAUNCH_MODE="bin"
    if ! API_BIN=$(ensure_api_bin); then
        return 1
    fi
    # Guard: never exec Windows PE under WSL (wrong binary → silent fail / wrong ports)
    if ! is_runnable_native_bin "$API_BIN"; then
        echo -e "  ${RED}✗${NC} Refusing non-native API binary: $API_BIN"
        echo -e "  ${DIM}Under WSL, build Linux binary: cargo build --release -p ghost-link${NC}"
        return 1
    fi
    echo -e "  ${DIM}API binary: ${API_BIN}${NC}"

    (
        cd "$PROJECT_ROOT"
        "$API_BIN" serve "$BACKEND_HOST" "$BACKEND_PORT"
    ) >/tmp/ghostlink_api.log 2>&1 &
    API_PID=$!

    # First boot after cargo build can be slow on /mnt/c
    local api_wait=90
    if is_wsl; then
        api_wait=180
    fi
    if ! wait_for_http "http://${BACKEND_HOST}:${BACKEND_PORT}/health" "Ghostlink API" "$api_wait" "inference_backend" "GHOSTLINK_API_PORT"; then
        echo -e "  ${RED}✗${NC} API failed — see /tmp/ghostlink_api.log"
        echo -e "  ${DIM}Tip: ensure port ${BACKEND_PORT} is free and ghost-link is a Linux binary under WSL.${NC}"
        tail -40 /tmp/ghostlink_api.log 2>/dev/null || true
        return 1
    fi
    if ! wait_for_http "http://${BACKEND_HOST}:${BACKEND_PORT}/api/health" "API /api/health" 30 "inference_backend" "GHOSTLINK_API_PORT"; then
        echo -e "  ${RED}✗${NC} /api/health failed — wrong process on :${BACKEND_PORT} or outdated binary"
        echo -e "  ${DIM}Check: curl -i http://${BACKEND_HOST}:${BACKEND_PORT}/api/health${NC}"
        tail -40 /tmp/ghostlink_api.log 2>/dev/null || true
        return 1
    fi

    # Verify critical GUI routes (GET). 405 here means wrong server (e.g. POST-only proxy).
    local code
    for path in /api/settings /api/models; do
        code=$(curl -s -o /dev/null -w "%{http_code}" "http://${BACKEND_HOST}:${BACKEND_PORT}${path}" || echo "000")
        if [ "$API_LAUNCH_MODE" = "bin" ] && { [ "$code" = "404" ] || [ "$code" = "405" ]; }; then
            echo -e "  ${YELLOW}⚠${NC} GET ${path} returned ${code} from prebuilt API binary; retrying with cargo run"

            [ -n "${API_PID:-}" ] && kill "$API_PID" 2>/dev/null || true
            wait "$API_PID" 2>/dev/null || true
            free_port "$BACKEND_PORT"

            (
                cd "$PROJECT_ROOT"
                cargo run -p ghost-link -- serve "$BACKEND_HOST" "$BACKEND_PORT"
            ) >/tmp/ghostlink_api.log 2>&1 &
            API_PID=$!
            API_LAUNCH_MODE="cargo"

            if ! wait_for_http "http://${BACKEND_HOST}:${BACKEND_PORT}/health" "Ghostlink API (cargo fallback)" 90 "inference_backend" "GHOSTLINK_API_PORT"; then
                echo -e "  ${RED}✗${NC} API failed after cargo fallback — see /tmp/ghostlink_api.log"
                return 1
            fi
            if ! wait_for_http "http://${BACKEND_HOST}:${BACKEND_PORT}/api/health" "API /api/health (cargo fallback)" 30 "inference_backend" "GHOSTLINK_API_PORT"; then
                echo -e "  ${RED}✗${NC} /api/health failed after cargo fallback"
                return 1
            fi

            # A passing health check above is not proof the NEW process answered
            # it: if the old process was never actually killed (e.g. free_port
            # had no way to find it, or it was orphaned by shell subshell
            # semantics), the new `cargo run` fails to bind the port, exits, and
            # every health check up to here would have kept talking to the OLD
            # process the whole time — which then still 404s on any route it
            # predates. Catch that here with a clear diagnostic instead of
            # letting it surface as a confusing later 404.
            if ! kill -0 "$API_PID" 2>/dev/null; then
                echo -e "  ${RED}✗${NC} cargo-fallback API process exited — port ${BACKEND_PORT} is likely still held by a stale process"
                echo -e "  ${DIM}Health checks above answered from that stale process, not the rebuild. See /tmp/ghostlink_api.log${NC}"
                echo -e "  ${DIM}Fix: manually stop whatever holds port ${BACKEND_PORT} (e.g. sudo lsof -i:${BACKEND_PORT}) and relaunch.${NC}"
                tail -20 /tmp/ghostlink_api.log 2>/dev/null || true
                return 1
            fi

            code=$(curl -s -o /dev/null -w "%{http_code}" "http://${BACKEND_HOST}:${BACKEND_PORT}${path}" || echo "000")
        fi
        if [ "$code" = "405" ]; then
            echo -e "  ${RED}✗${NC} GET ${path} returned 405 Method Not Allowed"
            echo -e "  ${DIM}Port ${BACKEND_PORT} is not ghost-link. Free the port and relaunch.${NC}"
            return 1
        fi
        if [ "$code" != "200" ]; then
            echo -e "  ${RED}✗${NC} GET ${path} failed (HTTP ${code})"
            return 1
        fi
    done

    # Chat endpoint must accept POST (not 404/405). Empty body → 4xx is OK; 405 is not.
    code=$(curl -s -o /dev/null -w "%{http_code}" -X POST \
        -H "Content-Type: application/json" \
        -d '{}' \
        "http://${BACKEND_HOST}:${BACKEND_PORT}/api/inference/chat" || echo "000")
    if [ "$code" = "405" ] || [ "$code" = "404" ]; then
        echo -e "  ${RED}✗${NC} POST /api/inference/chat returned HTTP ${code}"
        echo -e "  ${DIM}Wrong API process on :${BACKEND_PORT}. Rebuild: cargo build --release -p ghost-link${NC}"
        return 1
    fi
    echo -e "  ${GREEN}✓${NC} API routes verified (health, settings, models, chat POST)"
    echo ""

    # 3. React Frontend — always targets ghost-link API base above
    echo -e "  ${WHITE}▶${NC} ${BOLD}React Frontend (Vite)${NC} ${DIM}(port ${GUI_PORT})${NC}"
    progress_bar 0 3
    echo " ${DIM}Checking dependencies...${NC}"

    local NODE_BIN=""
    if ! NODE_BIN=$(resolve_node_bin); then
        echo -e "  ${RED}✗${NC} Native Node.js not found (Windows node.exe via WSL interop is not supported)"
        echo -e "  ${DIM}Install in WSL: curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash - && sudo apt-get install -y nodejs${NC}"
        echo -e "  ${DIM}Or use Windows launcher: .\\launch.bat${NC}"
        return 1
    fi
    local NPM_BIN
    NPM_BIN="$(dirname "$NODE_BIN")/npm"
    if [ ! -x "$NPM_BIN" ]; then
        if command -v npm >/dev/null 2>&1 && is_runnable_native_bin "$(command -v npm)"; then
            NPM_BIN=$(command -v npm)
        else
            echo -e "  ${RED}✗${NC} npm not found next to native node at $NODE_BIN"
            return 1
        fi
    fi
    echo -e "  ${DIM}Node: ${NODE_BIN}${NC}"

    cd "$PROJECT_ROOT/ghostlink_gui_modern" || return 1
    # Windows node_modules (from launch.bat) lack Linux optional natives (e.g. @rollup/rollup-linux-x64-gnu)
    local need_npm_install=0
    local platform_marker=".ghostlink-npm-platform"
    local want_platform
    want_platform="$(host_os_family)-$(uname -m)"
    if [ ! -d "node_modules" ]; then
        need_npm_install=1
    elif [ "$(cat "$platform_marker" 2>/dev/null || true)" != "$want_platform" ]; then
        need_npm_install=1
        echo -e "  ${YELLOW}node_modules platform mismatch (need ${want_platform}) — reinstalling...${NC}"
    elif [ "$(host_os_family)" = "linux" ] \
        && [ ! -d "node_modules/@rollup/rollup-linux-x64-gnu" ] \
        && [ ! -d "node_modules/@rollup/rollup-linux-x64-musl" ]; then
        need_npm_install=1
        echo -e "  ${YELLOW}Missing Linux rollup binary — reinstalling npm packages...${NC}"
    fi
    if [ "$need_npm_install" -eq 1 ]; then
        progress_bar 1 3
        echo " ${YELLOW}Installing npm packages...${NC}"
        # Drop previous OS-specific tree so optional deps resolve for this host
        if [ -d "node_modules" ] && [ ! -L "node_modules" ]; then
            rm -rf node_modules
        fi
        PATH="$(dirname "$NODE_BIN"):$PATH" \
          "$NPM_BIN" install --legacy-peer-deps >/tmp/ghostlink_frontend_install.log 2>&1 \
          || {
            echo -e "  ${RED}✗${NC} npm install failed — see /tmp/ghostlink_frontend_install.log"
            tail -40 /tmp/ghostlink_frontend_install.log 2>/dev/null || true
            return 1
          }
        printf '%s\n' "$want_platform" >"$platform_marker"
    else
        progress_bar 1 3
        echo " ${GREEN}Dependencies cached${NC}         "
    fi

    progress_bar 2 3
    echo " ${DIM}Starting Vite dev server...${NC}"
    # Always pin GUI → ghost-link API (:8003). Never :8080/:11434.
    export VITE_GHOSTLINK_API_BASE="http://${BACKEND_HOST}:${BACKEND_PORT}"
    export VITE_PROXY_TARGET="http://${BACKEND_HOST}:${BACKEND_PORT}"
    # Clear any stale override that pointed at wrong ports
    unset VITE_GHOSTLINK_BACKEND_URL 2>/dev/null || true
    PATH="$(dirname "$NODE_BIN"):$PATH" \
      VITE_GHOSTLINK_API_BASE="http://${BACKEND_HOST}:${BACKEND_PORT}" \
      VITE_PROXY_TARGET="http://${BACKEND_HOST}:${BACKEND_PORT}" \
      "$NPM_BIN" run dev -- --host 127.0.0.1 --port "$GUI_PORT" >/tmp/ghostlink_frontend.log 2>&1 &
    GUI_PID=$!
    cd "$PROJECT_ROOT" || true

    if ! wait_for_http "http://127.0.0.1:${GUI_PORT}" "React Frontend" 60; then
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
    
    local gui_port="${GUI_PORT:-5173}"
    local api_port="${GHOSTLINK_API_PORT:-8003}"
    local llama_port="${GHOSTLINK_LLAMA_SERVER_PORT:-8080}"
    local inference="${GHOSTLINK_INFERENCE_BACKEND:-native}"

    echo -e "${BLUE}┌─ Service Endpoints ──────────────────────────────────────────────────────────────┐${NC}"
    echo -e "${BLUE}│${NC}  ${WHITE}▶${NC} ${BOLD}Web Interface${NC}      → ${CYAN}http://127.0.0.1:${gui_port}${NC}"
    echo -e "${BLUE}│${NC}  ${WHITE}▶${NC} ${BOLD}API Server${NC}         → ${CYAN}http://127.0.0.1:${api_port}${NC}"
    if [ "$inference" = "ollama" ]; then
        echo -e "${BLUE}│${NC}  ${WHITE}▶${NC} ${BOLD}Ollama${NC}             → ${CYAN}http://127.0.0.1:11434${NC}"
    else
        echo -e "${BLUE}│${NC}  ${WHITE}▶${NC} ${BOLD}Native Inference${NC}   → ${CYAN}http://127.0.0.1:${llama_port}${NC} ${DIM}(llama-server)${NC}"
    fi
    echo -e "${BLUE}└────────────────────────────────────────────────────────────────────────────────────┘${NC}"
    echo ""
    
    echo -e "${YELLOW}┌─ Quick Start ────────────────────────────────────────────────────────────────────┐${NC}"
    echo -e "${YELLOW}│${NC}  ${WHITE}1.${NC} Open ${CYAN}http://127.0.0.1:${gui_port}${NC} in your browser"
    echo -e "${YELLOW}│${NC}  ${WHITE}2.${NC} Go to ${BOLD}Models${NC} tab → Load a model (dynamic load/unload supported)"
    echo -e "${YELLOW}│${NC}  ${WHITE}3.${NC} Settings → switch inference_backend between ${BOLD}native${NC} and ${BOLD}ollama${NC}"
    echo -e "${YELLOW}│${NC}  ${WHITE}4.${NC} Switch to ${BOLD}Chat${NC} tab → Start talking"
    echo -e "${YELLOW}│${NC}  ${WHITE}5.${NC} Runtime: ${BOLD}GHOSTLINK_INFERENCE_BACKEND=ollama|native${NC} ./launch.sh"
    echo -e "${YELLOW}└────────────────────────────────────────────────────────────────────────────────────┘${NC}"
    echo ""
    
echo -e "  ${DIM}Hardware: ${GPU_NAME:-CPU} (${BACKEND})${NC}"
echo -e "  ${DIM}RAM: ${RAM_GB} GB | GPU Layers: ${LLAMA_NGL:-0} | Threads: ${THREADS:-auto}${NC}"
[[ -n "$NPU_DETECTED" ]] && echo -e "  ${DIM}NPU: $NPU_DETECTED${NC}"
echo ""
echo -e "${DIM}Runtime Configuration:${NC}"
echo -e "  ${DIM}• Inference backend: ${inference}${NC}"
echo -e "  ${DIM}• Native engine: ${GHOSTLINK_NATIVE_ENGINE:-llama_server}${NC}"
echo -e "  ${DIM}• GPU backend: ${BACKEND}${NC}"
[ -n "${MODEL_ALIAS:-}" ] && echo -e "  ${DIM}• Model: ${MODEL_ALIAS}${NC}"
[ -n "${GHOSTLINK_LLAMA_SERVER_BIN:-}" ] && echo -e "  ${DIM}• llama-server bin: ${GHOSTLINK_LLAMA_SERVER_BIN}${NC}"
[ "$BACKEND" = "rocm" ] && echo -e "  ${DIM}• HSA_OVERRIDE_GFX_VERSION: $HSA_OVERRIDE_GFX_VERSION${NC}"
[ "$BACKEND" = "rocm" ] && echo -e "  ${DIM}• HIP_PLATFORM: $HIP_PLATFORM${NC}"
echo ""
echo -e "${DIM}Press ${BOLD}Ctrl+C${NC} ${DIM}to stop all services${NC}"
echo -e "${DIM}Logs: /tmp/ghostlink_*.log${NC}"
echo ""
}

# Cleanup on exit
cleanup() {
    echo -e "\n${YELLOW}Shutting down services...${NC}"
    [ -n "${GUI_PID:-}" ] && kill $GUI_PID 2>/dev/null && wait $GUI_PID 2>/dev/null || true
    [ -n "${API_PID:-}" ] && kill $API_PID 2>/dev/null && wait $API_PID 2>/dev/null || true
    [ -n "${LLAMA_PID:-}" ] && kill $LLAMA_PID 2>/dev/null && wait $LLAMA_PID 2>/dev/null || true
    # Port-scoped cleanup only (see start_services) — never a blind system-wide pkill.
    free_port "${GHOSTLINK_API_PORT:-8003}" 2>/dev/null || true
    free_port "${GHOSTLINK_LLAMA_SERVER_PORT:-8080}" 2>/dev/null || true
    free_port "${GUI_PORT:-5173}" 2>/dev/null || true
    echo -e "${GREEN}✓${NC} All services stopped."
    echo -e "${SHOW_CURSOR}"
    tput cnorm 2>/dev/null
    exit 0
}

trap cleanup EXIT INT TERM

# Main cinematic sequence
main() {
    # Detect Git Bash / MSYS (not WSL) and redirect to launch.bat
    if [[ "$(uname -s)" =~ MINGW|MSYS|CYGWIN ]]; then
        echo -e "${YELLOW}Windows shell detected.${NC} Please use ${BOLD}launch.bat${NC} instead of launch.sh."
        echo -e "Run: ${CYAN}.\\launch.bat${NC}"
        exit 1
    fi

    if is_wsl; then
        echo -e "${CYAN}WSL detected (${WSL_DISTRO_NAME:-WSL2}).${NC} Using Linux binaries only (ignoring Windows .exe)."
        echo -e "${DIM}API must be http://127.0.0.1:8003 — not llama-server :8080.${NC}"
        echo -e "${DIM}For GPU/Windows-native stack, prefer: .\\launch.bat${NC}"
        echo ""
    fi
    
    echo -e "${HIDE_CURSOR}"
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
    echo -e "${BLUE}┌─ Hardware Detection ─────────────────────────────────────────────────────────────┐${NC}"
    detect_cpu_ram
    detect_gpu
    echo -e "${BLUE}└────────────────────────────────────────────────────────────────────────────────────┘${NC}"
    echo ""
    check_components
    start_services || { cleanup; exit 1; }
    show_success
    
    # Keep running until Ctrl+C
    wait
}

main "$@"