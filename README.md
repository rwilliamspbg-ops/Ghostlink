# Ghostlink Studio - Modern GUI

Advanced AI Model Management Interface with Real-time Metrics, Tool Integration, and MCP Server Support.

**Status**: ✅ Production Ready | **Version**: 1.0.0 | **Default GUI**: Modern Web-Based

---

## 🎯 Features

### Chat Interface
- **Model Selection** - Dropdown selector for all available models
- **Real-time Parameters** - Temperature, Top-P, Top-K, Penalty, Max Tokens
- **System Prompts** - Customize AI behavior per conversation
- **Tool Integration** - 8 built-in tools (web search, calculator, code execution, etc.)
- **MCP Servers** - Add custom MCP servers for extended capabilities
- **Live Streaming** - Real-time response generation

### Models Management
- **Local Models** - Load, unload, delete models
- **Model Filtering** - Smart filtering for usable chat models
- **HuggingFace Integration** - 10 popular models pre-loaded, searchable
- **One-Click Download** - Download directly from HuggingFace
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

### Option 1: Auto-Launch Everything (Recommended)

**Linux/macOS:**
```bash
bash launch-complete.sh
```

**Windows:**
```bash
launch-complete.bat
```

This will:
- ✅ Auto-detect and start backend (if binary exists)
- ✅ Install GUI dependencies
- ✅ Open browser automatically
- ✅ Load all models
- ✅ Start metrics dashboard

### Option 2: Docker Compose

```bash
cd ghostlink_gui_modern
docker-compose up
```

Access at:
- **GUI**: http://localhost:3000
- **Backend**: http://127.0.0.1:8003

### Option 3: Manual Start
```bash
# Terminal 1: Start backend
./ghostlink serve

# Terminal 2: Start GUI
cd ghostlink_gui_modern
npm install --legacy-peer-deps
npm run dev
```

---

## 📊 Architecture

### Frontend Stack
- **React 18** - UI framework
- **TypeScript** - Full type safety
- **Tailwind CSS** - Responsive styling
- **Zustand** - State management
- **Axios** - HTTP client
- **Vite 5** - Ultra-fast build tool

### Component Structure
```
src/components/
├── ChatTab.tsx          # Chat with tools & MCP
├── ModelsTab.tsx        # Model management
├── MetricsTab.tsx       # Live metrics dashboard
├── SessionsTab.tsx      # Session monitoring
├── WorkersTab.tsx       # Worker management
└── SecurityTab.tsx      # Security controls

src/
├── api.ts               # Typed API client
├── store.ts             # Zustand state
└── App.tsx              # Main component
```

---

## 🛠️ Tools & MCP

### Using Built-in Tools

1. Open **Chat** tab
2. Click "Show" under **Tools & MCP**
3. Check boxes for tools you need
4. Select a model
5. Type your prompt
6. Send message

**Example**: Enable `web_search` → Send "What's new in AI?" → Model searches web and includes results

### Adding Custom MCP Servers

1. Start your MCP server: `python mcp_server.py`
2. In Chat tab, click "Add" under MCP Servers
3. Enter:
   - **Name**: Friendly name (e.g., "Weather API")
   - **URL**: Server URL (e.g., `http://localhost:5000`)
4. Click Add
5. Enable server (checkbox)
6. Use in prompts

**Response includes**: "Tools used: web_search, calculator"

---

## 📈 Metrics Dashboard

**Real-time updates every 5 seconds:**

- **Throughput** - Requests/second (cyan gauge)
- **CPU Usage** - Percentage (orange gauge)
- **Memory Usage** - Percentage (purple gauge)
- **GPU Usage** - Percentage (green gauge)
- **Latency P50** - Milliseconds (yellow gauge)
- **Latency P95** - Milliseconds (red gauge)

Each gauge shows health status: Green (Healthy) → Yellow (Caution) → Red (Alert)

---

## 🐳 Docker Deployment

### Build Image
```bash
cd ghostlink_gui_modern
docker build -t ghostlink-gui .
```

### Run with Compose
```bash
docker-compose up -d
```

**Includes:**
- ✅ Backend container
- ✅ GUI container
- ✅ Auto-health checks
- ✅ Data persistence (models, logs)
- ✅ Auto-restart

---

## 📝 Configuration

### Backend URL
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

### Metrics Refresh Rate
Edit `src/components/MetricsTab.tsx`:
```typescript
setInterval(refreshMetrics, 5000); // Change to desired ms
```

### Add More Tools
Edit `src/components/ChatTab.tsx`:
```typescript
const AVAILABLE_TOOLS: Tool[] = [
  // Add tools here
];
```

---

## 🔍 Performance

- **App Load**: <2 seconds
- **Build Size**: 75 KB gzipped
- **Memory Usage**: 60-80MB active
- **CPU Overhead**: <2% idle
- **Metrics Refresh**: 5 seconds (real-time)

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
- All Tkinter references

---

## 🆘 Troubleshooting

### Models Not Showing
```bash
# Verify backend
curl http://127.0.0.1:8003/api/models

# Refresh browser and Models tab
```

### Port Already in Use
```bash
# Change port in vite.config.ts
# Or kill process: lsof -i :3000 | kill -9
```

### MCP Server Not Connecting
```bash
# Verify server running: curl http://localhost:5000
# Check URL format (no trailing slash)
# Check firewall/network
```

### Build Issues
```bash
cd ghostlink_gui_modern
rm -rf node_modules package-lock.json
npm install --legacy-peer-deps
npm run build
```

---

## ✅ Requirements

- **Node.js**: 18+
- **npm**: 9+
- **Browser**: Chrome 90+, Firefox 88+, Safari 14+, Edge 90+
- **Backend**: Ghostlink backend running on 127.0.0.1:8003

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
- Model verification: `python scripts/verify_hf_models.py`

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
- Chat with model/tool selection
- Models management (local + HuggingFace)
- Metrics dashboard (live gauges)
- Sessions monitoring
- Workers management
- Security vault

### Scripts (4)
- `launch-complete.sh` - Auto-launch (Linux/macOS)
- `launch-complete.bat` - Auto-launch (Windows)
- `ghostlink_gui_modern/launch-gui.sh` - GUI only
- `ghostlink_gui_modern/launch-gui.bat` - GUI only

### Docker (2)
- `Dockerfile` - GUI image
- `docker-compose.yml` - Complete stack

### Documentation (5)
- `README.md` - This file
- `CHANGELOG.md` - Version history
- `STARTUP_GUIDE.md` - Setup guide
- `TOOLS_AND_MCP_GUIDE.md` - Tool integration
- `QUICK_REFERENCE.md` - Commands

---

**The modern Ghostlink Studio GUI - Enterprise-grade AI model management. 🚀**

Get started with `bash launch-complete.sh` (Linux/macOS) or `launch-complete.bat` (Windows)
