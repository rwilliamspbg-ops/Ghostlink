#!/usr/bin/env python3
"""
Quick start guide for testing Ghostlink GUI with real LLM.
"""

def main():
    print("\n" + "="*60)
    print("  GHOSTLINK GUI TEST SETUP")
    print("="*60 + "\n")
    
    print("This setup tests the Ghostlink Studio GUI with a real LLM")
    print("using Ollama (tinyllama model) as the backend.\n")
    
    print("="*60)
    print("  REQUIREMENTS")
    print("="*60 + "\n")
    print("1. Python 3.9+")
    print("2. Ollama (https://ollama.com)")
    print("3. Docker & Docker Compose (optional)")
    print("4. ~500MB disk space (for tinyllama model)\n")
    
    print("="*60)
    print("  QUICK START (LOCAL)")
    print("="*60 + "\n")
    print("Step 1: Install Ollama")
    print("  $ curl https://ollama.com/install.sh | sh\n")
    
    print("Step 2: Start Ollama and pull tinyllama")
    print("  $ ollama run tinyllama")
    print("  (This will download ~405MB)\n")
    
    print("Step 3: Install Python dependencies")
    print("  $ python3 -m pip install -r requirements.txt\n")
    
    print("Step 4: Run the test suite")
    print("  $ bash scripts/test_gui_with_ollama.sh\n")
    
    print("Step 5 (optional): Start the GUI")
    print("  $ python3 ghostlink_gui.py\n")
    
    print("="*60)
    print("  QUICK START (DOCKER)")
    print("="*60 + "\n")
    print("One command to test everything:")
    print("  $ docker-compose -f docker-compose.gui-test.yml up\n")
    
    print("In another terminal:")
    print("  $ bash scripts/test_gui_with_ollama.sh\n")
    
    print("="*60)
    print("  WHAT THE TEST SUITE VERIFIES")
    print("="*60 + "\n")
    print("  [OK] Backend health & connectivity")
    print("  [OK] Real Ollama models loaded (no mock data)")
    print("  [OK] Chat with actual LLM responses")
    print("  [OK] Metrics, sessions, workers endpoints")
    print("  [OK] Model loading and downloading")
    print("  [OK] Security features (JWT, PQC)")
    print("  [OK] Concurrent request handling")
    print("  [OK] No mock/placeholder responses detected\n")
    
    print("="*60)
    print("  TROUBLESHOOTING")
    print("="*60 + "\n")
    
    print("[ERROR] 'Ollama not running on localhost:11434'")
    print("  -> Install and start Ollama: ollama run tinyllama\n")
    
    print("[ERROR] 'Backend did not start'")
    print("  -> Check logs: cat /tmp/ghostlink_backend.log")
    print("  -> Verify port 8003 is free\n")
    
    print("[ERROR] 'tinyllama model not found'")
    print("  -> Pull it: ollama pull tinyllama\n")
    
    print("[ERROR] GUI shows 'Backend offline'")
    print("  -> Ensure backend server is running:")
    print("    python3 scripts/backend_test_server.py")
    print("  -> Check: curl http://127.0.0.1:8003/health\n")
    
    print("="*60)
    print("  FILES CREATED")
    print("="*60 + "\n")
    print("requirements.txt")
    print("  - Python dependencies\n")
    
    print("scripts/backend_test_server.py")
    print("  - Test backend server (proxies to Ollama)\n")
    
    print("scripts/test_gui_functions.py")
    print("  - Automated test suite (15 tests)\n")
    
    print("scripts/test_gui_with_ollama.sh")
    print("  - Test orchestration script\n")
    
    print("docker-compose.gui-test.yml")
    print("  - Docker setup with Ollama + backend\n")
    
    print("Dockerfile.gui-test")
    print("  - Container image for backend\n")
    
    print("="*60)
    print("  ENVIRONMENT VARIABLES")
    print("="*60 + "\n")
    print("GHOSTLINK_BACKEND_HOST")
    print("  -> Backend listen address (default: 127.0.0.1)\n")
    
    print("GHOSTLINK_BACKEND_PORT")
    print("  -> Backend listen port (default: 8003)\n")
    
    print("OLLAMA_HOST")
    print("  -> Ollama server URL (default: http://127.0.0.1:11434)\n")
    
    print("="*60)
    print("  API ENDPOINTS TESTED")
    print("="*60 + "\n")
    print("GET  /health")
    print("GET  /api/models")
    print("POST /api/inference/chat (with real LLM)")
    print("GET  /api/metrics")
    print("GET  /api/sessions")
    print("GET  /api/workers")
    print("POST /api/workers/add")
    print("POST /api/security/jwt/refresh")
    print("POST /api/security/pqc/enable\n")
    
    print("="*60)
    print("  NEXT STEPS")
    print("="*60 + "\n")
    print("1. Run: bash scripts/test_gui_with_ollama.sh")
    print("2. Launch: python3 ghostlink_gui.py")
    print("3. Test chat tab with real LLM responses\n")

if __name__ == "__main__":
    main()
