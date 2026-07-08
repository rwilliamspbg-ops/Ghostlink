# Ghostlink Studio - Modern GUI

Advanced AI Model Management Interface with Real-time Metrics, Tool Integration, MCP Server Support, and **Real Model Inference via Ollama**.

**Status**: ✅ Production Ready | **Version**: 1.0.0 | **Default GUI**: Modern Web-Based | **Inference**: Real Models via Ollama

---

## 🎯 Features

### Chat Interface
- **Real Model Inference** - Get responses from actual LLMs (Mistral, Llama, etc.) via Ollama
- **Model Selection** - Dropdown selector for all available models
- **Real-time Parameters** - Temperature, Top-P, Top-K, Penalty, Max Tokens
- **System Prompts** - Customize AI behavior per conversation
- **Tool Integration** - 8 built-in tools (web search, calculator, code execution, etc.)
- **MCP Servers** - Add custom MCP servers for extended capabilities
- **Live Streaming** - Real-time response generation

### Models Management
- **Ollama Integration** - Download and manage models from Ollama registry
- **Local Models** - Load, unload, delete models from your system
- **Automatic Model Pull** - First run auto-downloads Mistral model (~2GB)
- **HuggingFace Support** - Access 10+ popular models
- **Status Display** - Real-time model status and statistics

### Live Metrics Dashboard
- **Real-time Gauges** - 6 digital gauges updating every 5 seconds
- **Throughput Monitoring** - Requests per second
- **Resource Usage** - CPU, Memory, GPU metrics
- **Latency Tracking** - P50 and P95 percentiles
- **Health Indicators** - Color-coded status (Healthy/Caution/Alert)

### Sessions & Workers
- **Session Monitoring** - Track active inference sessions
- **Worker Management** - Distributed worker node orchestration
- **Peer Discovery** - Auto-discover network peers
- **Load Balancing** - Monitor and manage load distribution
- **Network Health** - Real-time connectivity status

### Security
- **JWT Management** - Token refresh with countdown timer
- **Post-Quantum Cryptography** - PQC encryption support
- **Security Vault** - Digital vault interface
- **Audit Logging** - Comprehensive security event logs

### Tools & MCP

#### 8 Built-in Tools
1. **web_search** - Search the web for current information
2. **calculator** - Perform mathematical operations
3. **code_execution** - Execute Python safely in sandbox
4. **file_operations** - Read/write file system
5. **terminal** - Execute system commands
6. **database_query** - Query connected databases
7. **api_call** - Make HTTP API calls
8. **image_generation** - Generate and edit images

#### MCP Server Integration
- Add custom MCP servers via UI
- Enable/disable per conversation
- Connect to external services
- Extend model capabilities
- No configuration files needed

---

## 🚀 Quick Start

### Option 1: Auto-Launch Everything with Real Model Inference (Recommended)

**Linux/macOS:**
```bash
bash launch-complete.sh
```

**Windows:**
```bash
launch-complete.bat
```

This will automatically:
- ✅ Start Ollama (if installed) with real model inference
- ✅ Auto-detect and start Ghostlink backend
- ✅ Install GUI dependencies
- ✅ Open browser to http://localhost:3000
- ✅ Pull Mistral model on first run (~2GB)

**What happens next:**
1. Browser opens to http://localhost:3000
2. Go to **Models** tab → all available models load
3. Go to **Chat** tab → select a model
4. Type a message → get a real response from the model

### Option 2: Docker Compose (Complete Stack)

```bash
docker-compose -f docker-compose.production.yml up
```

Services start automatically:
- **Ollama** (Model Inference): http://localhost:11434
- **Backend** (API): http://127.0.0.1:8003
- **Frontend** (GUI): http://localhost:5174

### Option 3: Manual Start (One Terminal Per Service)

```bash
# Terminal 1: Start Ollama (real model inference)
ollama serve

# Terminal 2: Download a model
ollama pull mistral

# Terminal 3: Start Ghostlink backend
./ghostlink serve 0.0.0.0 8003

# Terminal 4: Start GUI
cd ghostlink_gui_modern
npm install --legacy-peer-deps
npm run dev
```

---

## 🧠 Model Inference

### How It Works
1. **Frontend** sends your message to **Backend** on port 8003
2. **Backend** connects to **Ollama** on port 11434
3. **Ollama** runs the selected model and generates response
4. **Response** streams back to your browser in real-time

### Supported Models
Download any model from Ollama registry:

```bash
ollama pull mistral         # 7B - fast & capable
ollama pull llama2          # 7B/13B - flexible
ollama pull neural-chat     # 7B - optimized for chat
ollama pull orca-mini       # 3B - lightweight
ollama pull openhermes      # 7B - instruction-tuned
```

List installed models:
```bash
ollama list
```

### Fallback Mode (No Ollama)
If Ollama isn't installed or running:
- Backend automatically falls back to mock responses
- UI still works, but responses are simulated
- Perfect for testing without models

---

## 📊 Architecture

### Stack
- **Inference Engine**: Ollama (llama.cpp backend, quantized models)
- **API Server**: Rust + Axum (http://localhost:8003)
- **Frontend**: React 18 + TypeScript + Vite (http://localhost:3000)
- **State**: Zustand (UI state management)
- **Styling**: Tailwind CSS

### Data Flow
```
User Input (React)
    ↓
[Chat Component]
    ↓
[Axios API call] → http://localhost:8003/api/inference/chat
    ↓
[Rust Backend]
    ↓
[Ollama Client] → http://localhost:11434/api/generate
    ↓
[llama.cpp] → Actual Model Inference
    ↓
Response streams back through same chain
```

### Component Structure
```
src/components/
├── ChatTab.tsx          # Chat with real model responses
├── ModelsTab.tsx        # Manage loaded models
├── MetricsTab.tsx       # Live performance gauges
├── SessionsTab.tsx      # Session monitoring
├── WorkersTab.tsx       # Worker management
└── SecurityTab.tsx      # Security controls

src/
├── api.ts               # Typed Axios client
├── store.ts             # Zustand state
└── App.tsx              # Main container
```

---

## 🛠️ Tools & MCP

### Using Built-in Tools

1. Open **Chat** tab
2. Click "Show" under **Tools & MCP**
3. Check boxes for tools (e.g., web_search, calculator)
4. Select a model
5. Type your prompt
6. Send message

**Example**: "Search for AI news and summarize"
- Model uses `web_search` tool → gets latest results → generates summary

### Adding Custom MCP Servers

1. Start your MCP server: `python mcp_server.py`
2. In Chat tab, click "Add" under MCP Servers
3. Enter:
   - **Name**: Friendly name (e.g., "Weather API")
   - **URL**: Server URL (e.g., `http://localhost:5000`)
4. Click Add → Server now available
5. Model uses server tools in responses

---

## 📈 Metrics Dashboard

Real-time updates every 5 seconds:

- **Throughput** - Requests/second (cyan gauge)
- **CPU Usage** - Percentage (orange gauge)
- **Memory Usage** - Percentage (purple gauge)
- **GPU Usage** - Percentage (green gauge)
- **Latency P50** - Milliseconds (yellow gauge)
- **Latency P95** - Milliseconds (red gauge)

Each gauge shows health status: **Green** (Healthy) → **Yellow** (Caution) → **Red** (Alert)

---

## 🐳 Docker Deployment

### Production Stack (Ollama + Backend + Frontend)

```bash
docker-compose -f docker-compose.production.yml up -d
```

Services:
- ✅ Ollama container with model caching
- ✅ Backend container with health checks
- ✅ Frontend container on port 5174
- ✅ Auto-restart on failure
- ✅ Data persistence (models, logs)

### Individual Images

```bash
# Build Ghostlink GUI image
cd ghostlink_gui_modern
docker build -t ghostlink-gui .

# Build Ollama image (or use official: ollama/ollama)
docker build -t ghostlink-ollama -f Dockerfile.ollama .
```

---

## 📝 Configuration

### Backend API URL
Edit `ghostlink_gui_modern/vite.config.ts`:
```typescript
server: {
  proxy: {
    '/api': {
      target: 'http://your-backend:8003',
      changeOrigin: true,
    },
  },
},
```

### Ollama Base URL
Automatically detected from `OLLAMA_BASE_URL` environment variable:
```bash
export OLLAMA_BASE_URL=http://ollama:11434
./ghostlink serve
```

### Metrics Refresh Rate
Edit `src/components/MetricsTab.tsx`:
```typescript
setInterval(refreshMetrics, 5000); // Change to desired ms
```

### Add More Models
In CLI:
```bash
ollama pull quantum
ollama pull medllama2
ollama pull starling-lm
```

---

## 🔍 Performance

- **App Load**: <2 seconds
- **Build Size**: 75 KB gzipped (frontend)
- **Memory Usage**: 60-80MB (frontend) + 2GB+ (model)
- **CPU Overhead**: <2% idle
- **Metrics Refresh**: 5 seconds (real-time)
- **Model Inference**: Depends on model size & CPU/GPU

### Recommended Setup for Smooth Performance
- **CPU**: 4+ cores (more = faster inference)
- **RAM**: 8GB+ (16GB+ if running large models)
- **Storage**: 10GB+ for models (Mistral = 4.1GB)
- **GPU**: Optional (CUDA/Metal accelerates 2-5x)

---

## 📚 Documentation

### Main Guides
- **STARTUP_GUIDE.md** - Complete setup and deployment
- **TOOLS_AND_MCP_GUIDE.md** - Tools and MCP server integration
- **CHANGELOG.md** - Version history and updates

### Reference
- **QUICK_REFERENCE.md** - Command reference card
- **INDEX.md** - Documentation index

### Archived (Legacy)
- `MIGRATION.md` - Tkinter to modern GUI migration
- `SETUP_GUIDE.md` - Old setup guide

---

## 🆘 Troubleshooting

### Models Not Showing
```bash
# Verify Ollama running
curl http://localhost:11434/api/tags

# Download a model
ollama pull mistral

# Refresh browser
```

### Backend Connection Failed
```bash
# Verify backend running
curl http://localhost:8003/health

# Check logs (macOS/Linux)
tail -f /tmp/ghostlink-backend.log
```

### Ollama Not Starting
```bash
# Install Ollama (macOS/Linux)
curl -fsSL https://ollama.ai/install.sh | sh

# Or download from https://ollama.ai

# On Windows: Download installer from https://ollama.ai/download
```

### Port Already in Use
```bash
# Kill process using port 8003
lsof -i :8003 | grep -v PID | awk '{print $2}' | xargs kill -9

# Or change port in launch script
```

### Models Taking Too Long
```bash
# Large models need time to download (Mistral = 4.1GB)
# Check progress: Monitor your internet speed

# Try smaller model
ollama pull orca-mini  # 1.7GB, faster
```

---

## ✅ Requirements

- **Node.js**: 18+
- **npm**: 9+
- **Ollama**: Latest (auto-installs with launch script suggestion)
- **Backend**: Ghostlink backend binary or compiled from source
- **RAM**: 8GB+ (16GB+ recommended for larger models)
- **Storage**: 10GB+ for models

### Optional
- **GPU**: CUDA-capable NVIDIA GPU (recommended for 2-5x speedup)
- **Metal**: Apple Silicon (auto-enabled)

---

## 📊 Browser Support

| Browser | Version | Status |
|---------|---------|--------|
| Chrome | 90+ | ✅ Full Support |
| Firefox | 88+ | ✅ Full Support |
| Safari | 14+ | ✅ Full Support |
| Edge | 90+ | ✅ Full Support |
| Mobile | iOS 14+, Android | ✅ Responsive |

---

## 🛠 Development & CI

The following commands are used for local validation and CI parity checks:

- Run tests: `cargo test --workspace`
- Lint check: `cargo clippy --workspace --all-targets -- -D warnings`
- Build image: `docker build -t ghostlink-gui ./ghostlink_gui_modern`
- Run compose: `docker-compose -f docker-compose.production.yml up`

---

## 🎓 Examples

### Web Research Task
```
1. Enable: web_search, code_execution
2. Prompt: "Research AI market size 2024, compare 2023, project 2025"
3. Model uses web_search → code_execution → generates analysis
```

### System Administration
```
1. Enable: terminal, file_operations, code_execution
2. Prompt: "Check disk usage, identify large files, generate cleanup"
3. Model uses all tools → generates automated script
```

### Data Analysis
```
1. Enable: api_call, database_query, code_execution
2. Prompt: "Fetch API data, query database, analyze trends"
3. Model integrates multiple sources → generates report
```

---

## 📞 Support

### Getting Started
1. Run `bash launch-complete.sh` (or `.bat` on Windows)
2. Wait for browser to open http://localhost:3000
3. Go to **Models** tab and verify model loaded
4. Go to **Chat** tab and send a test message
5. Check for real model response (not mock)

### Common Issues
See **STARTUP_GUIDE.md** Troubleshooting section

### Tool Integration
See **TOOLS_AND_MCP_GUIDE.md**

### Version History
See **CHANGELOG.md**

---

## 📄 License

Part of Ghostlink Studio - See LICENSE file

---

## 🎉 What's Included

### Components (6)
- Chat with real model inference
- Models management (Ollama + HuggingFace)
- Metrics dashboard (live gauges)
- Sessions monitoring
- Workers management
- Security vault

### Scripts (2)
- `launch-complete.sh` - Auto-launch all services (Linux/macOS)
- `launch-complete.bat` - Auto-launch all services (Windows)

### Docker (1)
- `docker-compose.production.yml` - Complete stack with Ollama

### Documentation (5)
- `README.md` - This file
- `CHANGELOG.md` - Version history
- `STARTUP_GUIDE.md` - Setup guide
- `TOOLS_AND_MCP_GUIDE.md` - Tool integration
- `QUICK_REFERENCE.md` - Commands

---

**The modern Ghostlink Studio GUI - Enterprise-grade AI model management with real model inference. 🚀**

Get started: `bash launch-complete.sh` (Linux/macOS) or `launch-complete.bat` (Windows)
