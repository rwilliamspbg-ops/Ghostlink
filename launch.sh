#!/usr/bin/env bash

# Ghostlink Studio - Cinematic Launch Experience
# Beautiful splash screen with real service verification

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
    ╔════════════════════════════════════════════════════════════════════════════════════╗
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
    ║             ██║   ███████╗██║  ██║██║  ██║███████╗██║  ██║███████║                 ║
    ║             ╚═╝   ╚══════╝╚═╝  ╚═╝╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝╚══════╝                 ║
    ║                                                                                     ║
    ╚═══════════════════════════════════════════════════════════════════════════════════╝
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
    echo -e "${BLUE}└────────────────────────────────────────────────────────────────────────────────────┘${NC}"
    echo ""
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

# Animated service startup
start_services() {
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
    
    # Check if llama.cpp exists and build if needed
    if [ ! -x "$LLAMA_SERVER_BIN" ] && [ ! -f "$LLAMA_SERVER_BIN" ]; then
        progress_bar 1 3
        echo " ${YELLOW}Building llama-server...${NC}      "
        build_llama_cpp $IS_WINDOWS >/tmp/ghostlink-bootstrap.log 2>&1 &
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
    echo -e "  ${WHITE}▶${NC} ${BOLD}Ghostlink API Server${NC} ${DIM}(port 8003)${NC}"
    progress_bar 0 2
    echo " ${DIM}Starting...${NC}"
    
    # Start llama-server
    LLAMA_NGL="${LLAMA_NGL:--1}"
    mkdir -p "$PROJECT_ROOT/models"
    if [ ! -f "$PROJECT_ROOT/models/stories15M-q4_0.gguf" ]; then
        echo -e "  ${DIM}Downloading model...${NC}"
        curl -L --fail -o "$PROJECT_ROOT/models/stories15M-q4_0.gguf" \
            "https://huggingface.co/ggml-org/models/resolve/main/tinyllamas/stories15M-q4_0.gguf" \
            2>/dev/null || echo "  ${YELLOW}⚠ Model download failed, will use existing if any${NC}"
    fi
    
    "$LLAMA_SERVER_BIN" \
        -m "$PROJECT_ROOT/models/stories15M-q4_0.gguf" \
        --host 127.0.0.1 --port 8080 \
        -ngl "$LLAMA_NGL" \
        >/tmp/ghostlink_llama_server.log 2>&1 &
    LLAMA_PID=$!
    
    progress_bar 1 2
    echo " ${DIM}Waiting for llama-server...${NC}"
    wait_for_http "http://127.0.0.1:8080/health" "llama-server" 60
    
    # Start Ghostlink API
    GHOSTLINK_INFERENCE_BACKEND=native \
    GHOSTLINK_NATIVE_ENGINE=llama_server \
    GHOSTLINK_LLAMA_SERVER_URL="http://127.0.0.1:8080/completion" \
    bash -c "
        if [ -f '$PROJECT_ROOT/target/release/ghost-link' ]; then
            '$PROJECT_ROOT/target/release/ghost-link' serve 127.0.0.1 8003
        elif [ -f '$PROJECT_ROOT/target/debug/ghost-link' ]; then
            '$PROJECT_ROOT/target/debug/ghost-link' serve 127.0.0.1 8003
        else
            cargo run -p ghost-link -- serve 127.0.0.1 8003
        fi
    " \
        >/tmp/ghostlink_api.log 2>&1 &
    API_PID=$!
    
    wait_for_http "http://127.0.0.1:8003/health" "Ghostlink API" 60
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
    npm run dev -- --host 127.0.0.1 --port 5173 >/tmp/ghostlink_frontend.log 2>&1 &
    GUI_PID=$!
    cd ..
    
    wait_for_http "http://127.0.0.1:5173" "React Frontend" 60
    progress_bar 3 3
    echo " ${GREEN}Frontend ready${NC}              "
    echo ""
    
    echo -e "${BLUE}└────────────────────────────────────────────────────────────────────────────────────┘${NC}"
    echo ""
}

# Cross-platform llama.cpp builder
build_llama_cpp() {
    local IS_WINDOWS=$1
    local LLAMA_DIR="third_party/llama.cpp"
    
    # Check for cmake
    if ! command -v cmake >/dev/null 2>&1; then
        echo "ERROR: cmake not found in PATH" >&2
        echo "Please install cmake:" >&2
        echo "  Linux:    sudo apt-get install cmake" >&2
        echo "  macOS:    brew install cmake" >&2
        echo "  Windows:  winget install Kitware.CMake (or install from cmake.org)" >&2
        return 1
    fi
    
    # Check for git
    if ! command -v git >/dev/null 2>&1; then
        echo "ERROR: git not found in PATH" >&2
        return 1
    fi
    
    # Clone if needed
    if [ ! -d "$LLAMA_DIR" ]; then
        mkdir -p third_party
        git clone https://github.com/ggml-org/llama.cpp.git "$LLAMA_DIR" >/dev/null 2>&1
    fi
    
    cd "$LLAMA_DIR"
    
    # Detect GPU vendor for cmake flags
    local CMAKE_GPU_FLAGS=""
    if command -v nvidia-smi >/dev/null 2>&1 && nvidia-smi --version >/dev/null 2>&1; then
        echo "  NVIDIA GPU detected — building with CUDA support"
        CMAKE_GPU_FLAGS="-DLLAMA_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES=all"
    elif command -v rocm-smi >/dev/null 2>&1 && rocm-smi --version >/dev/null 2>&1; then
        echo "  AMD GPU detected — building with HIP/ROCm support"
        CMAKE_GPU_FLAGS="-DLLAMA_HIPBLAS=ON -DAMDGPU_TARGETS=all"
    elif command -v lspci >/dev/null 2>&1; then
        local amd_check
        amd_check=$(lspci -mm 2>/dev/null | grep -iE "VGA|3D" | grep -iE "amd|radeon")
        if [ -n "$amd_check" ]; then
            echo "  AMD GPU detected (lspci) — building with HIP/ROCm support"
            CMAKE_GPU_FLAGS="-DLLAMA_HIPBLAS=ON -DAMDGPU_TARGETS=all"
        fi
    fi

    if [ -z "$CMAKE_GPU_FLAGS" ]; then
        echo "  No GPU detected — building CPU-only llama.cpp"
    fi
    
    if [ $IS_WINDOWS -eq 1 ]; then
        cmake -S . -B build -DCMAKE_BUILD_TYPE=Release $CMAKE_GPU_FLAGS >/dev/null 2>&1
        cmake --build build --config Release --target llama-server --parallel >/dev/null 2>&1
    else
        cmake -S . -B build -DCMAKE_BUILD_TYPE=Release $CMAKE_GPU_FLAGS >/dev/null 2>&1
        cmake --build build --config Release --target llama-server --parallel >/dev/null 2>&1
    fi
    
    local result=$?
    cd - >/dev/null
    return $result
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
    echo -e "${BLUE}└────────────────────────────────────────────────────────────────────────────────────┘${NC}"
    echo ""
    
    echo -e "${YELLOW}┌─ Quick Start ────────────────────────────────────────────────────────────────────┐${NC}"
    echo -e "${YELLOW}│${NC}  ${WHITE}1.${NC} Open ${CYAN}http://127.0.0.1:5173${NC} in your browser"
    echo -e "${YELLOW}│${NC}  ${WHITE}2.${NC} Go to ${BOLD}Models${NC} tab → Select a model"
    echo -e "${YELLOW}│${NC}  ${WHITE}3.${NC} Switch to ${BOLD}Chat${NC} tab → Start talking!"
    echo -e "${YELLOW}│${NC}  ${WHITE}4.${NC} Watch real-time inference with native GPU acceleration"
    echo -e "${YELLOW}└────────────────────────────────────────────────────────────────────────────────────┘${NC}"
    echo ""
    
    echo -e "${DIM}Press ${BOLD}Ctrl+C${NC} ${DIM}to stop all services${NC}"
    echo -e "${DIM}Logs: /tmp/ghostlink_*.log${NC}"
    echo ""
}

# GPU detection — supports NVIDIA, AMD ROCm, Intel
detect_gpu() {
    local gpu_name=""
    local gpu_vendor=""

    if command -v nvidia-smi >/dev/null 2>&1; then
        gpu_name=$(nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null | head -1)
        if [ -n "$gpu_name" ]; then
            echo -e "  ${GREEN}╡${NC} ${WHITE}GPU${NC}          ${GREEN}NVIDIA: $gpu_name${NC}"
            gpu_vendor="nvidia"
        fi
    fi

    if [ -z "$gpu_vendor" ] && command -v rocm-smi >/dev/null 2>&1; then
        gpu_name=$(rocm-smi --showproductname 2>/dev/null | grep "Card model:" | head -1 | awk -F': ' '{print $3}')
        if [ -n "$gpu_name" ]; then
            echo -e "  ${GREEN}╡${NC} ${WHITE}GPU${NC}          ${GREEN}AMD: $gpu_name${NC}"
            gpu_vendor="amd"
        else
            echo -e "  ${GREEN}╡${NC} ${WHITE}GPU${NC}          ${GREEN}AMD ROCm detected${NC}"
            gpu_vendor="amd"
        fi
    fi

    if [ -z "$gpu_vendor" ]; then
        # lspci fallback for AMD/Intel GPUs
        if command -v lspci >/dev/null 2>&1; then
            local amd_gpu
            amd_gpu=$(lspci -mm 2>/dev/null | grep -iE "(VGA compatible|3D controller)" | grep -iE "amd|radeon|advanced micro" | head -1)
            if [ -n "$amd_gpu" ]; then
                gpu_name=$(echo "$amd_gpu" | awk -F'"' '{print $6}')
                echo -e "  ${GREEN}╡${NC} ${WHITE}GPU${NC}          ${GREEN}AMD: ${gpu_name:-AMD GPU}${NC}"
                gpu_vendor="amd"
            fi
        fi
    fi

    if [ -z "$gpu_vendor" ]; then
        echo -e "  ${YELLOW}╡${NC} ${WHITE}GPU${NC}          ${YELLOW}No GPU detected - using CPU mode${NC}"
        if [ "${LLAMA_NGL:--1}" = "-1" ]; then
            LLAMA_NGL=0
        fi
    fi
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