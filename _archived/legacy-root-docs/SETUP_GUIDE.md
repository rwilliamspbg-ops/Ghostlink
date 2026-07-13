# Ghostlink Studio - Complete Setup Guide

## 🚀 Quick Start (30 seconds)

### Windows
```bash
cd ghostlink_gui_modern
launch-gui.bat
```
**Browser opens automatically → http://localhost:3000**

### Linux / macOS
```bash
cd ghostlink_gui_modern
bash launch-gui.sh
```
**Browser opens automatically → http://localhost:3000**

---

## 📋 System Requirements

- **Node.js**: 18.0.0+
- **npm**: 9.0.0+ (included with Node.js)
- **Browser**: Chrome/Edge 90+, Firefox 88+, Safari 14+
- **OS**: Windows 10+, macOS 10.15+, Linux (any distro)

### Verify Installation
```bash
node --version    # Should be v18.x or higher
npm --version     # Should be 9.x or higher
```

---

## 🔧 Installation Steps

### 1. Prerequisites
Install Node.js from https://nodejs.org/ (LTS version recommended)

### 2. Install GUI Dependencies
```bash
cd ghostlink_gui_modern
npm install --legacy-peer-deps
```

### 3. Verify Installation
```bash
npm run build
```
Should complete without errors.

---

## 🎮 Running the GUI

### Development Mode (Recommended for testing)
```bash
cd ghostlink_gui_modern
npm run dev
```
- Opens http://localhost:3000
- Hot-reload on file changes
- Full debugging support

### Production Build
```bash
cd ghostlink_gui_modern
npm run build
npm run preview
```
- Optimized performance
- ~200KB gzipped
- Production-ready assets in `/dist`

### With Docker
```bash
cd ghostlink_gui_modern
docker build -t ghostlink-gui .
docker run -p 3000:3000 ghostlink-gui
```

---

## 🔌 Backend Configuration

### Default Connection
The GUI connects to `http://127.0.0.1:8003` by default.

### Change Backend URL

**Option 1: Edit vite.config.ts**
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

**Option 2: Environment Variable**
```bash
# Create .env.local
echo "VITE_API_BASE=http://your-backend:8003" > .env.local
```

**Option 3: CLI Argument** (future)
```bash
npm run dev -- --backend http://your-backend:8003
```

---

## 📊 Features Overview

### Chat Tab
- ✅ Model selection dropdown
- ✅ Real-time parameter control
- ✅ System prompt customization
- ✅ Message history

### Models Tab
- ✅ Local model browser
- ✅ Load/Unload models
- ✅ Delete models
- ✅ HuggingFace integration
- ✅ Model search and download

### Metrics Tab
- ✅ Digital gauge display
- ✅ Real-time updates
- ✅ Auto-refresh every 5s
- ✅ Health indicators

### Sessions Tab
- ✅ Active session list
- ✅ Session statistics
- ✅ Cancel sessions
- ✅ Real-time monitoring

### Workers Tab
- ✅ Worker management
- ✅ Network discovery
- ✅ Peer connectivity
- ✅ Health monitoring
- ✅ Load visualization

### Security Tab
- ✅ Digital vault interface
- ✅ JWT token management
- ✅ PQC encryption toggle
- ✅ Security audit logging
- ✅ Status indicators

---

## 🐛 Troubleshooting

### GUI won't start
```bash
# Check Node.js
node --version

# Reinstall dependencies
rm -rf node_modules package-lock.json
npm install --legacy-peer-deps

# Start with verbose output
npm run dev -- --host 0.0.0.0
```

### Can't connect to backend
```bash
# Test backend connectivity
curl http://127.0.0.1:8003/health

# Check firewall
# Windows: netstat -ano | findstr :8003
# Linux: sudo netstat -tlnp | grep :8003

# Update backend URL in vite.config.ts
```

### Models not loading
```bash
# Check browser console for errors (F12)
# Verify backend /api/models endpoint
curl http://127.0.0.1:8003/api/models

# Clear browser cache
# Hard refresh: Ctrl+Shift+R
```

### High CPU/Memory usage
```bash
# Use production build instead of dev
npm run build
npm run preview

# Check for console errors
# Reduce refresh interval in components
```

### Port 3000 already in use
```bash
# Linux/macOS: Find and kill process
lsof -i :3000
kill -9 <PID>

# Windows: Find and kill process
netstat -ano | findstr :3000
taskkill /PID <PID> /F

# Or use different port
npm run dev -- --port 3001
```

---

## 📦 Deployment

### Standalone Docker
```bash
cd ghostlink_gui_modern
docker build -t ghostlink-gui:latest .
docker run -p 3000:3000 ghostlink-gui:latest
```

### Docker Compose (with Backend)
```bash
cd ghostlink_gui_modern
docker-compose up -d
```

### Kubernetes
```bash
kubectl apply -f k8s/deployment.yaml
```

### Systemd Service (Linux)
Create `/etc/systemd/system/ghostlink-gui.service`:
```ini
[Unit]
Description=Ghostlink Studio GUI
After=network.target

[Service]
Type=simple
User=www-data
WorkingDirectory=/opt/ghostlink/ghostlink_gui_modern
ExecStart=/usr/bin/npm run preview
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
```

Then:
```bash
sudo systemctl daemon-reload
sudo systemctl enable ghostlink-gui
sudo systemctl start ghostlink-gui
```

---

## 🏗️ Development

### Project Structure
```
ghostlink_gui_modern/
├── src/
│   ├── components/       # React components
│   ├── api.ts           # Backend API client
│   ├── store.ts         # Zustand state
│   ├── App.tsx          # Main app
│   └── index.css        # Tailwind
├── public/              # Static assets
├── dist/                # Production build
├── vite.config.ts       # Vite config
├── tailwind.config.js   # Tailwind config
├── tsconfig.json        # TypeScript config
└── package.json         # Dependencies
```

### Adding a New Component
1. Create `src/components/NewTab.tsx`
2. Export from `src/App.tsx`
3. Add to tabs array
4. Style with Tailwind CSS

### Adding API Methods
```typescript
// In src/api.ts
async newMethod(param: string) {
  try {
    const res = await this.http.get('/api/endpoint', { params: { param } });
    return { success: true, data: res.data };
  } catch (error: any) {
    return { success: false, error: error.message };
  }
}
```

### Building UI Components
```typescript
// Use existing patterns
- StatusMessage for feedback
- DigitalGauge for metrics
- Tables for data display
- Buttons for actions
```

---

## 🔍 Performance Tips

1. **Use Production Build** - 5-10x faster than dev
2. **Enable Browser Caching** - Set cache headers
3. **Lazy Load Tabs** - Load on demand with React.lazy
4. **Optimize Images** - Use WebP format
5. **Monitor Memory** - Check browser DevTools

---

## 🔒 Security

- ✅ HTTPS-ready (use with nginx/reverse proxy)
- ✅ CSP headers compatible
- ✅ XSS protection via React sanitization
- ✅ CSRF token support (add to backend)
- ✅ Secure JWT handling

### Production Security
```nginx
# nginx example
server {
    listen 443 ssl http2;
    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;
    
    location / {
        proxy_pass http://localhost:3000;
        proxy_http_version 1.1;
    }
    
    location /api {
        proxy_pass http://localhost:8003;
    }
}
```

---

## 📈 Monitoring

Monitor GUI performance:
```bash
# Check resource usage
top -p $(pgrep -f "npm run dev")

# Monitor network traffic
netstat -i

# Check error logs
npm run build 2>&1 | tee build.log
```

---

## 🆘 Getting Help

1. **Check logs**: Browser console (F12) and server logs
2. **Test endpoint**: `curl http://localhost:8003/api/models`
3. **Verify connectivity**: `ping 127.0.0.1:8003`
4. **Report issue**: Include Node version, OS, browser, error message

---

## 📝 Configuration Files

### package.json
- Dependencies and build scripts
- Development vs production configs

### vite.config.ts
- Build tool configuration
- API proxy settings
- Development server config

### tailwind.config.js
- Theme colors
- Responsive breakpoints
- Plugin configuration

### tsconfig.json
- TypeScript compiler options
- Module resolution
- Type checking

---

## 🚀 Next Steps

1. **Start Development**: `npm run dev`
2. **Explore Components**: Browse source code
3. **Customize Theme**: Edit `tailwind.config.js`
4. **Add Features**: Create new components
5. **Deploy**: Build and deploy with Docker

---

## 📞 Support Resources

- Documentation: `./GUI_README.md`
- Issues: GitHub issue tracker
- Discussions: GitHub discussions
- Docs: https://ghostlink-docs.dev

---

**Version**: 1.0.0  
**Updated**: 2024  
**Status**: Production Ready ✅
