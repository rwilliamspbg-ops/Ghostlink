# CHANGELOG

All notable changes to Ghostlink Studio Modern GUI are documented here.

---

## [Unreleased]

### Changed

- Retired duplicate, unused command and API stub modules under `crates/ghost-link/src/cli/` and `crates/ghost-link/src/api/`.
- Standardized CLI command source of truth on `crates/ghost-link/src/main.rs` to prevent behavioral drift.

### CI/Quality

- Added `scripts/verify_no_stub_todos.sh` and integrated it into CI workflows to fail builds when `TODO: Implement actual` markers are reintroduced in `crates/ghost-link/src`.

### Performance

- Re-validated tool-heavy chat endpoint performance after stub-surface cleanup and hot-path optimizations.

---

## [1.0.0] - 2024 (Current Release)

### ✨ Features

#### Chat Tab
- Model selector dropdown (filters usable models only)
- Real-time parameter controls (Temperature, Top-P, Top-K, Penalty, Max Tokens)
- System prompt customization
- **NEW**: 8 built-in tools integration
- **NEW**: Custom MCP server support
- Live streaming responses

#### Models Tab
- Browse local models
- Load/Unload/Delete operations
- Real-time status display
- **NEW**: HuggingFace integration (10 popular models pre-loaded)
- Search and filter capabilities
- One-click download from HuggingFace
- Model details (size, type, quantization, status)

#### Metrics Tab
- **NEW**: Live digital gauge dashboard
- 6 real-time metrics updating every 5 seconds
- Throughput (requests/second)
- CPU, Memory, GPU usage
- Latency P50 and P95
- Color-coded health indicators (Green/Yellow/Red)
- Raw JSON data display
- Smooth SVG animations

#### Sessions Tab
- Active session monitoring
- Real-time statistics
- Cancel sessions capability
- Session details display

#### Workers Tab
- Worker node management
- Add workers (host:port)
- Peer discovery functionality
- Network health monitoring
- Load visualization
- Disconnect workers
- Online/offline status tracking

#### Security Tab
- Digital vault interface
- JWT token management with countdown timer
- Post-Quantum Cryptography (PQC) support
- Security level indicator
- Comprehensive audit logging
- Security recommendations

#### Tools & MCP Support
- **NEW**: 8 built-in tools:
  - web_search
  - calculator
  - code_execution
  - file_operations
  - terminal
  - database_query
  - api_call
  - image_generation
- **NEW**: Custom MCP server integration
- Enable/disable tools per prompt
- Add/remove MCP servers via UI
- Tool execution tracking
- Response includes "Tools used" information

### 🚀 Launch & Deployment

#### Auto-Launch Scripts
- **NEW**: `launch-complete.sh` - One-command startup (Linux/macOS)
- **NEW**: `launch-complete.bat` - One-command startup (Windows)
- Backend auto-detection
- Dependency auto-install
- Browser auto-open
- Service URL display

#### Docker Compose
- **NEW**: Complete production stack
- Backend container integration
- GUI container orchestration
- Health checks
- Data persistence (volumes)
- Auto-restart policies
- Network isolation

### 🏗️ Architecture

#### Frontend
- React 18 with TypeScript
- Tailwind CSS styling
- Zustand state management
- Vite 5 build tool
- 100% type-safe codebase

#### API Client
- Typed HTTP requests
- Error handling
- Fallback mechanisms
- Tool/MCP payload support

#### Components
- Modular design (6 tabs)
- Hot reload support
- Responsive layout
- Mobile-friendly

### 📊 Performance

- Build size: 75 KB gzipped
- App load time: <2 seconds
- Memory usage: 60-80MB
- CPU overhead: <2% idle
- Metrics refresh: 5 seconds real-time

### 🔧 Configuration

- Backend URL configuration (vite.config.ts)
- Metrics refresh rate customization
- Tool additions supported
- MCP server management via UI

### 📚 Documentation

- **README.md** - Feature overview
- **STARTUP_GUIDE.md** - Complete setup guide
- **TOOLS_AND_MCP_GUIDE.md** - Tool integration guide
- **QUICK_REFERENCE.md** - Command reference
- **INDEX.md** - Documentation index
- **CHANGELOG.md** - This file

### 🔒 Security

- Sandboxed tool execution
- File operation restrictions
- Safe command subset
- Rate-limited API calls
- MCP server validation
- No secrets in frontend code

### 🎯 Models

- Smart filtering (status: ready, type: chat/LLM)
- All 4 backend models auto-populated
- 10 HuggingFace models pre-loaded
- Case-insensitive status matching
- Type mapping (LLM → chat)

### 🐛 Fixes

- Fixed model status matching (Ready → ready)
- Fixed model type mapping (LLM recognized)
- Icon import errors resolved (PlugOff → X, LockOpen → Unlock)
- Launch script path correction
- Auto-model fetching on app load

### 📦 Archived (Legacy)

- Tkinter GUI (`ghostlink_gui_tkinter.py`)
- Old migration guides
- Deprecated launcher scripts
- Legacy UI references

---

## Features by Category

### Chat Capabilities ✅
- [x] Model selection
- [x] Parameter tuning
- [x] System prompts
- [x] Tool integration
- [x] MCP servers
- [x] Live responses

### Model Management ✅
- [x] Load/unload
- [x] Delete models
- [x] Local browsing
- [x] HuggingFace search
- [x] One-click download
- [x] Status display

### Monitoring ✅
- [x] Live metrics (6 gauges)
- [x] 5-second refresh
- [x] Health indicators
- [x] Session tracking
- [x] Worker monitoring
- [x] Network health

### Tools ✅
- [x] 8 built-in tools
- [x] Tool selection UI
- [x] MCP servers
- [x] Tool execution
- [x] Response tracking

### Deployment ✅
- [x] Auto-launch scripts
- [x] Docker image
- [x] Docker Compose
- [x] Health checks
- [x] Data persistence

### Security ✅
- [x] JWT management
- [x] PQC support
- [x] Audit logging
- [x] Security vault
- [x] Sandboxed execution

---

## API Endpoints Supported

```
GET  /health                          ✅
GET  /api/models                      ✅
POST /api/models/load                 ✅
POST /api/models/download             ✅
POST /api/models/{name}/unload        ✅
DELETE /api/models/{name}             ✅
POST /api/inference/chat              ✅
GET  /api/metrics                     ✅
GET  /api/sessions                    ✅
POST /api/sessions/{id}/cancel        ✅
GET  /api/workers                     ✅
POST /api/workers/add                 ✅
POST /api/workers/connect             ✅
GET  /api/workers/discover            ✅
POST /api/security/jwt/refresh        ✅
POST /api/security/pqc/enable         ✅
```

---

## Browser Compatibility

| Browser | Min Version | Status |
|---------|------------|--------|
| Chrome | 90 | ✅ Full |
| Firefox | 88 | ✅ Full |
| Safari | 14 | ✅ Full |
| Edge | 90 | ✅ Full |
| Mobile | iOS 14+ | ✅ Responsive |

---

## Node.js Requirements

- **Node.js**: 18.0.0+
- **npm**: 9.0.0+

---

## Known Limitations

- MCP servers must be accessible from client
- Tool execution timeout: varies by tool
- File operations limited to designated directories
- Code execution: Python sandbox (60s timeout, 512MB memory)

---

## Migration from Tkinter

The old Tkinter GUI has been replaced entirely. If migrating from v0.x:

1. All functionality now in modern web GUI
2. Use `launch-complete.sh` or `launch-complete.bat`
3. Tools & MCP are new features
4. All endpoints remain compatible
5. No backend changes required

See `_archived/MIGRATION.md` for detailed comparison.

---

## Roadmap

### Future Versions

- [ ] WebSocket real-time updates (vs polling)
- [ ] Model versioning/rollback
- [ ] Advanced filtering/sorting
- [ ] User preferences/themes
- [ ] Export metrics to CSV/JSON
- [ ] API key management UI
- [ ] Rate limiting dashboard
- [ ] Model comparison tools
- [ ] Advanced logging/debugging
- [ ] Multi-user support

---

## Version History

| Version | Date | Status |
|---------|------|--------|
| 1.0.0 | 2024 | ✅ Current |
| 0.x | - | ❌ Archived |

---

## Credits

Built with:
- React 18
- TypeScript
- Tailwind CSS
- Vite
- Zustand
- Axios

---

## License

See LICENSE file

---

**Status**: ✅ Production Ready  
**Last Updated**: 2024  
**Maintainer**: Ghostlink Team
