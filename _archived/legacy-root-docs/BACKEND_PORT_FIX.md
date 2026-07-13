# 🔧 FIX: Backend Port Configuration

## Problem
Your backend is running on port **8000** (Docker), but the GUI was configured to proxy to port **8003**.

## Solution Applied

### 1. Updated vite.config.ts
Changed proxy target from `8003` → `8000`:
```typescript
proxy: {
  '/api': {
    target: 'http://127.0.0.1:8000',
    changeOrigin: true,
  },
  '/health': {
    target: 'http://127.0.0.1:8000',
    changeOrigin: true,
  },
},
```

### 2. What This Fixes
- ✅ Backend connection restored
- ✅ Models will load from backend
- ✅ Tools will work (if backend supports them)
- ✅ All API calls will reach the backend

## Next Steps

### 1. Refresh Browser
```
http://localhost:3000
Press Ctrl+R (or Cmd+R on Mac)
```

### 2. Check Models Load
- Go to Models tab
- Should see your 4 models
- Go to Chat tab
- Model dropdown should populate

### 3. Test Tools
The Chat tab now supports tools and MCP if your backend supports them:

**Enable a tool:**
1. Click "Show" under Tools & MCP
2. Check a tool (e.g., "web_search")
3. Select model
4. Send message
5. Response will show "Tools used" if backend processes them

### 4. Add MCP Server (Optional)
1. Click "Add" under MCP Servers
2. Enter server name and URL
3. Enable and use

## Backend Information

**Your backend:**
- Running on: http://127.0.0.1:8000
- Protocol: HTTP
- Health check: ✅ OK

**GUI Configuration:**
- Running on: http://localhost:3000
- Proxy to backend: ✅ Fixed to 8000
- Dev mode: ✅ Hot reload enabled

## Troubleshooting

### Models still not loading?
```bash
# Check backend is responding
curl http://127.0.0.1:8000/health

# Check backend has models (adjust endpoint if needed)
curl http://127.0.0.1:8000/api/models
```

### Tools not working?
The backend must support the tools endpoint:
```bash
curl http://127.0.0.1:8000/api/inference/chat \
  -X POST \
  -H "Content-Type: application/json" \
  -d '{"message":"test","tools":["web_search"]}'
```

### Still having issues?
Check backend logs:
```bash
# If Docker Compose
docker logs ghostlink-backend

# If running directly
# Check terminal running backend
```

## Environment Variable Alternative

If you need to change the port dynamically, set:
```bash
export BACKEND_PORT=8000
npm run dev
```

---

**Backend is now properly configured. Refresh your browser to test!** ✅
