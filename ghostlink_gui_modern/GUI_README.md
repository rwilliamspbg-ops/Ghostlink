# Ghostlink Studio - Modern GUI

**Advanced AI Model Management Interface** - The new default GUI for all Ghostlink scripts.

## Quick Start

### Windows
```bash
cd ghostlink_gui_modern
launch-gui.bat
```

### Linux / macOS
```bash
cd ghostlink_gui_modern
bash launch-gui.sh
```

## Features

### 🎯 Chat Tab
- **Model Selection** - Dropdown selector for all usable models
- **Real-time Parameters** - Temperature, Top-P, Top-K, Penalty controls
- **System Prompts** - Customize AI behavior
- **Live Responses** - Stream chat responses in real-time

### 📚 Models Tab
**Local Models:**
- Browse all available models with smart filtering
- Load/Unload models on demand
- Delete models to free up space
- View model details (size, type, quantization, status)

**Hugging Face Integration:**
- Search HF model hub directly
- Download models to local system
- View popularity metrics (likes, downloads)
- One-click model download and loading

### 📊 Metrics Tab
**Digital Gauge Dashboard:**
- **Throughput** - Requests per second (cyan)
- **CPU Usage** - CPU utilization percentage (orange)
- **Memory** - RAM utilization (purple)
- **GPU Usage** - GPU utilization (green)
- **Latency P50** - 50th percentile latency (yellow)
- **Latency P95** - 95th percentile latency (red)

Each gauge features:
- Real-time SVG animation
- Health status indicators (Healthy/Caution/Alert)
- Color-coded status (Green/Yellow/Red)
- Auto-refresh every 5 seconds

### 🔗 Workers Tab
**Network Management:**
- Add workers manually with host/port
- Automatic peer discovery
- Real-time worker health monitoring
- Network connection status

**Worker Monitoring:**
- Online/Offline status tracking
- Load balancing visualization
- Thread count monitoring
- Active model display
- Network health indicators

**Operations:**
- Connect/Disconnect workers
- Discover peers on the network
- Monitor total system load

### 🔐 Security Tab
**Digital Vault Interface:**
- JWT token management with countdown timer
- Post-Quantum Cryptography (PQC) toggle
- Security level indicator (Maximum/Standard)
- Comprehensive audit logging
- Security status summary

**Features:**
- Auto-expiration alerts for JWT
- PQC enablement timestamp tracking
- Real-time security recommendations
- Colored status indicators

### 🏥 Health Dashboard
- Backend connectivity status
- Real-time uptime display
- Current active model
- Connection health indicators

## Installation

### Prerequisites
- Node.js 18+ ([download](https://nodejs.org/))
- npm (comes with Node.js)

### Setup
```bash
cd ghostlink_gui_modern
npm install --legacy-peer-deps
```

## Development

### Start Dev Server
```bash
npm run dev
```
Open `http://localhost:3000`

### Build for Production
```bash
npm run build
```

### Production Preview
```bash
npm run preview
```

## Environment Variables

Create a `.env.local` file to override defaults:
```
VITE_API_BASE=http://your-backend:8003
```

## Docker Deployment

### Build
```bash
docker build -t ghostlink-gui .
```

### Run
```bash
docker run -p 3000:3000 \
  -e GHOSTLINK_API_BASE=http://backend:8003 \
  ghostlink-gui
```

### Docker Compose
```bash
docker-compose up
```

## Architecture

### Frontend Stack
- **React 18** - UI framework
- **TypeScript** - Type safety
- **Tailwind CSS** - Responsive styling
- **Zustand** - State management
- **Axios** - HTTP client
- **Vite** - Ultra-fast build tool

### Component Structure
```
src/
├── components/
│   ├── ChatTab.tsx          # Chat interface
│   ├── ModelsTab.tsx        # Model management + HF
│   ├── MetricsTab.tsx       # Digital gauge dashboard
│   ├── SessionsTab.tsx      # Session management
│   ├── WorkersTab.tsx       # Network & peer management
│   ├── SecurityTab.tsx      # Security vault
│   └── StatusIndicator.tsx  # Health indicator
├── api.ts                   # Backend API client
├── store.ts                 # Zustand state
├── App.tsx                  # Main app component
└── index.css               # Tailwind styles
```

## API Integration

The GUI connects to backend at `http://127.0.0.1:8003` by default.

### Proxied Endpoints
- `/api/*` - Model, inference, worker APIs
- `/health` - Backend health check

## Browser Support
- Chrome/Edge 90+
- Firefox 88+
- Safari 14+
- Mobile browsers (iOS 14+, Android Chrome)

## Performance

- **Dev Mode**: Hot Module Replacement (HMR) for instant updates
- **Production**: Optimized build (~200KB gzipped)
- **Real-time**: Auto-refresh every 5 seconds for metrics
- **Responsive**: Mobile-optimized at all breakpoints

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Enter` | Send message (Chat tab) |
| `Enter` | Search HuggingFace (Models tab) |
| `Ctrl+K` | Search functionality (future) |

## Troubleshooting

### GUI won't connect to backend
1. Verify backend is running on `http://127.0.0.1:8003`
2. Check proxy settings in `vite.config.ts`
3. Ensure CORS headers are set on backend

### Models not showing
1. Refresh the Models tab
2. Verify backend `/api/models` endpoint
3. Check browser console for errors

### High CPU usage
1. Reduce refresh interval in component
2. Use production build (`npm run build`)
3. Check for console errors

## Improvements Over Tkinter

| Feature | Tkinter | Modern GUI |
|---------|---------|-----------|
| UI Framework | tkinter | React 18 |
| Styling | Basic colors | Modern dark theme |
| Responsiveness | Fixed layout | Fully responsive |
| Type Safety | None | Full TypeScript |
| Performance | Slow | Fast (Vite) |
| Mobile Support | None | Full support |
| Model Filtering | Basic | Smart filtering |
| Metrics Display | Text | Digital gauges |
| Security UI | Basic | Digital vault |
| Network Discovery | None | Peer discovery |
| Hot Reload | None | Full HMR |

## Development Guide

### Adding a New Feature

1. **Create component** in `src/components/NewFeature.tsx`
2. **Add to store** in `src/store.ts` if needed
3. **Add API methods** in `src/api.ts`
4. **Update App.tsx** to include in tabs
5. **Style with Tailwind** - all CSS is in JSX

### Adding API Endpoints

```typescript
// In src/api.ts
async newEndpoint(param: string) {
  try {
    const response = await this.http.post('/api/endpoint', { param });
    return { success: true, data: response.data };
  } catch (error: any) {
    return { success: false, error: error.message };
  }
}
```

## License

Part of Ghostlink Studio - see main repo LICENSE

## Support

For issues, feature requests, or feedback:
1. Check existing GitHub issues
2. Create detailed bug report
3. Include browser/OS information
4. Provide error logs from console

---

**Made with ❤️ for advanced AI model management**
