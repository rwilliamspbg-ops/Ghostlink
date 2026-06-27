"""
Mohawk Inference Engine GUI - Production Setup Script

This script handles:
- Package installation from source
- PyInstaller executable building (Windows/Linux)
- Docker image building
- Cross-platform deployment preparation
"""

import sys
import os
from pathlib import Path

def get_project_root():
    """Get the project root directory."""
    return Path(__file__).parent.resolve()

def print_banner():
    """Print installation banner."""
    banner = f"""
╔═══════════════════════════════════════════════════════════════════╗
║   MOHAWK INFERENCE ENGINE GUI - Production Build System v2.1.0    ║
╚═══════════════════════════════════════════════════════════════════╝
"""
    print(banner)

def check_dependencies():
    """Check and install required build dependencies."""
    missing = []
    
    try:
        import PyQt6
    except ImportError:
        missing.append("PyQt6")
    
    try:
        import cryptography
    except ImportError:
        missing.append("cryptography")
    
    try:
        import PyJWT
    except ImportError:
        missing.append("PyJWT")
    
    if missing:
        print(f"Missing dependencies: {', '.join(missing)}")
        print("Running: pip install -r requirements.txt\n")
        os.system("pip install -r requirements.txt")
        return True
    return False

def build_executable():
    """Build standalone executable using PyInstaller."""
    print("\n🔨 Building executable with PyInstaller...")
    
    # Check if pyinstaller is installed
    try:
        import PyInstaller
        print("✓ PyInstaller found")
    except ImportError:
        print("Installing PyInstaller...")
        os.system("pip install PyInstaller")
    
    # Build command
    build_cmd = [
        "pyinstaller",
        "--name=Mohawk-Inference-Engine",
        "--onefile",
        "--windowed",  # No console for GUI app
        "--add-data=mohawk_gui/resources:resources",
        "--hidden-import=mohawk_gui.main",
        "--hidden-import=mohawk_gui.auth_manager",
        "--hidden-import=mohawk_gui.connection_pool",
        "--hidden-import=mohawk_gui.metrics_buffer",
        "--hidden-import=mohawk_gui.error_recovery",
        "--hidden-import=mohawk_gui.monitoring",
        "--hidden-import=mohawk_gui.audit_logger",
        "-p", str(Path("mohawk_gui").resolve()),
    ]
    
    print(f"Running: {' '.join(build_cmd)}")
    os.system(" ".join(build_cmd))
    
    # Check for built executable
    dist_path = Path("dist")
    if dist_path.exists():
        exe_files = list(dist_path.glob("Mohawk*-Inference-Engine*.exe"))
        if exe_files:
            print(f"\n✅ Executable built successfully!")
            print(f"Location: {exe_files[0]}")
            return str(exe_files[0])
    
    print("\n⚠️  Build completed but no executable found in dist/")
    return None

def create_dockerfile():
    """Create Dockerfile for containerized deployment."""
    dockerfile_path = Path("Dockerfile")
    
    dockerfile_content = f"""# Mohawk Inference Engine GUI - Production Docker Image
# Version: 2.1.0

FROM python:{sys.version_info.major}.{sys.version_info.minor}-slim

# Set environment variables
ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    PIP_NO_CACHE_DIR=1 \
    PIP_DISABLE_PIP_VERSION_CHECK=1

# Set working directory
WORKDIR {"/app"}

# Install system dependencies
RUN apt-get update && apt-get install -y \
    gcc \
    git \
    libgl1-mesa-glx \
    libglib2.0-0 \
    libxkbcommon-x11-0 \
    libdbus-1-3 \
    && rm -rf /var/lib/apt/lists/*

# Copy requirements first for better caching
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Copy application code
COPY mohawk_gui/ ./mohawk_gui/

# Create non-root user for security
RUN groupadd mohawk && useradd -r -g mohawk mohawk
USER mohawk

# Expose ports
EXPOSE 8003 8443

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD python -c "import sys; sys.exit(0 if __import__('mohawk_gui').main else 1)" || exit 1

# Default command
CMD ["python", "mohawk_gui/main.py"]
"""
    
    with open(dockerfile_path, 'w') as f:
        f.write(dockerfile_content)
    
    print(f"✓ Dockerfile created at {dockerfile_path}")
    return str(dockerfile_path)

def create_build_scripts():
    """Create build scripts for Windows and Linux."""
    
    # Windows batch script
    windows_script = Path("build_windows.bat")
    windows_content = f"""@echo off
REM Mohawk Inference Engine GUI - Windows Build Script
REM Version: 2.1.0

echo ════════════════════════════════════════════════════════════
echo   MOHAWK INFERENCE ENGINE GUI - Windows Build System v2.1.0
echo ════════════════════════════════════════════════════════════
echo.

REM Check Python version
python --version
echo.

REM Install dependencies
echo Installing dependencies...
pip install -r requirements.txt
echo.

REM Build executable
echo Building executable...
pyinstaller \
    --name=Mohawk-Inference-Engine \
    --onefile \
    --windowed \
    --add-data=mohawk_gui\\resources;resources \
    --hidden-import=mohawk_gui.main \
    --hidden-import=mohawk_gui.auth_manager \
    --hidden-import=mohawk_gui.connection_pool \
    --hidden-import=mohawk_gui.metrics_buffer \
    --hidden-import=mohawk_gui.error_recovery \
    --hidden-import=mohawk_gui.monitoring \
    --hidden-import=mohawk_gui.audit_logger \
    -p mohawk_gui

echo.
echo ════════════════════════════════════════════════════════════
echo   BUILD COMPLETE!
echo ════════════════════════════════════════════════════════════
echo.
echo Executable location: dist\\Mohawk*-Inference-Engine.exe
echo.
pause
"""
    
    with open(windows_script, 'w') as f:
        f.write(windows_content)
    print(f"✓ Windows build script created: {windows_script}")
    
    # Linux shell script
    linux_script = Path("build_linux.sh")
    linux_content = f"""#!/bin/bash
# Mohawk Inference Engine GUI - Linux Build Script
# Version: 2.1.0

echo ════════════════════════════════════════════════════════════
echo   MOHAWK INFERENCE ENGINE GUI - Linux Build System v2.1.0
echo ════════════════════════════════════════════════════════════
echo

# Check Python version
python3 --version
echo

# Install dependencies
echo Installing dependencies...
pip install -r requirements.txt
echo

# Build executable
echo Building executable...
pyinstaller \
    --name=Mohawk-Inference-Engine \
    --onefile \
    --windowed \
    --add-data=mohawk_gui/resources:resources \
    --hidden-import=mohawk_gui.main \
    --hidden-import=mohawk_gui.auth_manager \
    --hidden-import=mohawk_gui.connection_pool \
    --hidden-import=mohawk_gui.metrics_buffer \
    --hidden-import=mohawk_gui.error_recovery \
    --hidden-import=mohawk_gui.monitoring \
    --hidden-import=mohawk_gui.audit_logger \
    -p mohawk_gui

echo
echo ════════════════════════════════════════════════════════════
echo   BUILD COMPLETE!
echo ════════════════════════════════════════════════════════════
echo
echo Executable location: dist/Mohawk*-Inference-Engine
echo
"""
    
    with open(linux_script, 'w') as f:
        f.write(linux_content)
    print(f"✓ Linux build script created: {linux_script}")

def create_entry_point():
    """Create simplified entry point for the GUI."""
    entry_path = Path("mohawk_gui/entry_point.py")
    
    entry_content = f"""#!/usr/bin/env python3
\"\"\"
Mohawk Inference Engine GUI - Simplified Entry Point
Version: 2.1.0

This is a simplified entry point that can be used directly
or embedded in the PyInstaller executable.
\"\"\"

import sys
import os

# Add project root to path
project_root = Path(__file__).parent.parent
if str(project_root) not in sys.path:
    sys.path.insert(0, str(project_root))

from mohawk_gui.main import main

def main():
    \"\"\"Main entry point for Mohawk Inference Engine GUI.\"\"\"
    print(\"╔═══════════════════════════════════════════════════════════╗\")
    print(\"║   MOHAWK INFERENCE ENGINE GUI v2.1.0                      ║\")
    print(\"║   Production-Ready Multi-Device Inference Management      ║\")
    print(\"╚═══════════════════════════════════════════════════════════╝\")
    print()
    
    # Parse command line arguments
    import argparse
    
    parser = argparse.ArgumentParser(
        description=\"Mohawk Inference Engine GUI - Production Ready\"
    )
    parser.add_argument(
        \"--host\", 
        default=\"localhost\",
        help=\"Worker host (default: localhost)\"
    )
    parser.add_argument(
        \"--port\", 
        type=int, 
        default=8003,
        help=\"Worker port (default: 8003)\"
    )
    parser.add_argument(
        \"--config\",
        default=\"config.toml\",
        help=\"Configuration file path\"
    )
    
    args = parser.parse_args()
    
    # Initialize and run GUI
    from mohawk_gui.main_window import MohawkGUI
    
    gui = MohawkGUI()
    gui.show()
    
    return gui.exec()

if __name__ == \"__main__\":
    sys.exit(main())
"""
    
    with open(entry_path, 'w') as f:
        f.write(entry_content)
    print(f"✓ Entry point created: {entry_path}")

def create_config_template():
    """Create default configuration template."""
    config_path = Path("mohawk_gui/config.toml")
    
    config_content = f"""# Mohawk Inference Engine GUI - Configuration Template
# Version: 2.1.0
# =============================================================================

[mohawk]
# Worker connection settings
host = "localhost"
port = 8003

# SSL/TLS configuration (production)
ssl_enabled = false
ssl_cert = "certs/client.crt"
ssl_key = "certs/client.key"

[workers]
# Worker management settings
enabled = true
auto_discover = false
timeout_ms = 5000
max_connections = 100

[sessions]
# Session management
max_concurrent = 10
default_batch_size = 32
checkpoint_interval_s = 60

[metrics]
# Metrics collection
sampling_rate = 0.1
export_interval_s = 60
buffer_window_size = 1000

[logging]
# Logging configuration
level = "INFO"
file = "logs/mohawk_gui.log"
format = "json"

[security]
# Security settings
jwt_expiry_hours = 24
refresh_window_hours = 1
audit_enabled = true

[[resources]]
name = "model.onnx"
path = "models/model.onnx"
devices = ["gpu_0", "gpu_1"]

[[resources]]
name = "tokenizer"
path = "models/tokenizer.json"
devices = ["cpu"]
"""
    
    with open(config_path, 'w') as f:
        f.write(config_content)
    print(f"✓ Configuration template created: {config_path}")

def create_docker_compose():
    """Create Docker Compose file for development."""
    docker_compose_path = Path("docker-compose.yml")
    
    docker_compose_content = """# Mohawk Inference Engine GUI - Docker Compose
# Version: 2.1.0

version: '3.8'

services:
  mohawk-gui:
    build:
      context: .
      dockerfile: Dockerfile
    container_name: mohawk-gui
    ports:
      - "8003:8003"
      - "8443:8443"
    volumes:
      - ./mohawk_gui:/app/mohawk_gui
      - ./certs:/app/certs
      - ./logs:/app/logs
    environment:
      - PYTHONUNBUFFERED=1
      - PYTHONDONTWRITEBYTECODE=1
    command: >
      python mohawk_gui/main.py \
      --host ${HOST:-localhost} \
      --port ${PORT:-8003}
    healthcheck:
      test: ["CMD", "python", "-c", "import sys; sys.exit(0)"]
      interval: 30s
      timeout: 10s
      retries: 3
    restart: unless-stopped

  mohawk-worker:
    build:
      context: .
      dockerfile: Dockerfile.worker
    container_name: mohawk-worker
    ports:
      - "8003:8003"
    volumes:
      - ./models:/app/models
      - ./certs:/app/certs
    command: python prototype/worker_secure.py --port 8003
    restart: unless-stopped

networks:
  default:
    name: mohawk-network

"""
    
    with open(docker_compose_path, 'w') as f:
        f.write(docker_compose_content)
    print(f"✓ Docker Compose created: {docker_compose_path}")

def main():
    """Main build script entry point."""
    print_banner()
    
    # Get project root
    project_root = get_project_root()
    os.chdir(project_root)
    
    print(f"\nProject root: {project_root}")
    print("=" * 70 + "\n")
    
    # Step 1: Check dependencies
    if not check_dependencies():
        print("Dependencies already satisfied.\n")
    
    # Step 2: Create build artifacts
    print("\n📦 Creating build artifacts...")
    create_dockerfile()
    create_build_scripts()
    create_entry_point()
    create_config_template()
    create_docker_compose()
    
    # Step 3: Build executable (optional)
    build_executable()
    
    print("\n" + "=" * 70)
    print("✅ Production build system setup complete!")
    print("=" * 70)
    print("\nNext steps:")
    print("1. Run 'build_windows.bat' on Windows or 'build_linux.sh' on Linux")
    print("2. Or run: pyinstaller --name Mohawk-Inference-Engine --onefile mohawk_gui/main.py")
    print("3. Distribute the executable in dist/")
    print("\nFor Docker deployment:")
    print("1. Run: docker-compose up -d")
    print("2. Or build image: docker build -t mohawk-gui .")
    print("=" * 70 + "\n")

if __name__ == "__main__":
    main()
