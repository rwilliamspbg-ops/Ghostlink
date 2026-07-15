# 🔧 Ghostlink Production Fixes - Implementation Guide

## Summary

Ghostlink backend is **fully functional**. Frontend has **3 critical bugs** preventing chat/worker functionality. All fixes are surgical, low-risk changes.

**Time to fix: ~15 minutes**

---

## Critical Fix #1: ChatTab Empty Message Bug

**Location:** `ghostlink_gui_modern/src/components/ChatTab.tsx` (Line 61-115)

**Problem:** Input message cleared BEFORE sending to API, so backend receives empty string

**Symptom:** Chat appears to work but backend gets no message text

**Fix:**
```typescript
// BEFORE (BROKEN):
setMessages((prev) => [...prev, userMessage]);
setInput('');  // ← Cleared here
setLoading(true);
const result = await api.sendMessage({
  message: input,  // ← Now empty!
  ...
});

// AFTER (FIXED):
const messageText = input.trim();  // Capture first
if (!messageText || loading) return;

const userMessage: Message = {
  role: 'user',
  content: messageText,
  ...
};

setMessages((prev) => [...prev, userMessage]);
setInput('');  // Clear AFTER
setLoading(true);
const result = await api.sendMessage({
  message: messageText,  // Use captured text
  ...
});
```

**File to update:** `ghostlink_gui_modern/src/components/ChatTab.tsx` → Use provided `ChatTab.tsx` (already fixed)

---

## Critical Fix #2: WorkersTab Polling Missing

**Location:** `ghostlink_gui_modern/src/components/WorkersTab.tsx` (useEffect)

**Problem:** Worker list fetched only once on mount, never updates

**Symptom:** Worker load/status changes not reflected; UI becomes stale

**Fix:**
```typescript
// BEFORE (BROKEN):
useEffect(() => {
  refreshWorkers();
}, [api]);  // Only runs once

// AFTER (FIXED):
useEffect(() => {
  refreshWorkers();
  
  const interval = setInterval(() => {
    refreshWorkers();
  }, 5000);  // Poll every 5 seconds

  return () => clearInterval(interval);
}, [api, setWorkers]);
```

**File to update:** `ghostlink_gui_modern/src/components/WorkersTab.tsx` → Use provided `WorkersTab_FIXED.tsx`

---

## Critical Fix #3: WorkersTab Missing Disconnect Handler

**Location:** `ghostlink_gui_modern/src/components/WorkersTab.tsx` (Line ~65)

**Problem:** Power button has no click handler; disconnect endpoint never called

**Symptom:** Clicking "disconnect" does nothing

**Fix:**
```typescript
// BEFORE (BROKEN):
<button className="p-2 text-slate-500 hover:text-red-400 transition">
  <Power size={20} />
</button>

// AFTER (FIXED):
const handleDisconnectWorker = async (workerId: string) => {
  const result = await api.disconnectWorker(workerId);
  if (result.success) {
    refreshWorkers();
  }
};

<button 
  onClick={() => handleDisconnectWorker(worker.id)}
  className="p-2 text-slate-500 hover:text-red-400 hover:bg-red-500/10 rounded-lg transition"
  title="Disconnect worker"
>
  <Power size={20} />
</button>
```

**File to update:** `ghostlink_gui_modern/src/components/WorkersTab.tsx` → Use provided `WorkersTab_FIXED.tsx`

---

## Configuration Fix: Backend Discovery

**Location:** `ghostlink_gui_modern/src/App.tsx`

**Problem:** `apiBase` initialized empty; frontend can't find backend

**Symptom:** API calls fail silently; no backend connection established

**Fix:**
```typescript
// BEFORE (BROKEN):
const [api] = useState(() => new GhostlinkAPI(apiBase));  // apiBase is ''

// AFTER (FIXED):
const [api, setApi] = useState<GhostlinkAPI | null>(null);

useEffect(() => {
  const detectedApiBase = import.meta.env.VITE_API_URL || 'http://localhost:8003';
  setApiBase(detectedApiBase);
  setApi(new GhostlinkAPI(detectedApiBase));
}, [setApiBase]);
```

**File to update:** `ghostlink_gui_modern/src/App.tsx` → Use provided `App_FIXED.tsx`

---

## Configuration Fix: Vite Proxy

**Location:** `ghostlink_gui_modern/vite.config.ts` (new file)

**Problem:** No proxy configured; CORS blocks frontend→backend requests

**Symptom:** API calls may fail with CORS errors

**Fix:** Create `vite.config.ts`:
```typescript
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: 'http://localhost:8003',
        changeOrigin: true,
      },
      '/health': {
        target: 'http://localhost:8003',
        changeOrigin: true,
      },
      '/v1': {
        target: 'http://localhost:8003',
        changeOrigin: true,
      }
    }
  },
})
```

**File:** `ghostlink_gui_modern/vite.config.ts` (provided)

---

## Environment Configuration

**Location:** `ghostlink_gui_modern/.env.example` (new file)

**Add** these environment variable options:
```
VITE_API_URL=http://localhost:8003
VITE_API_TIMEOUT=5000
VITE_API_STREAM_TIMEOUT=120000
VITE_WORKER_POLL_INTERVAL=5000
VITE_DEBUG=false
```

**File:** `ghostlink_gui_modern/.env.example` (provided)

---

## Implementation Checklist

- [ ] **Step 1:** Backup current `ghostlink_gui_modern/src/components/ChatTab.tsx`
- [ ] **Step 2:** Replace with provided `ChatTab.tsx` (already fixed in this session)
- [ ] **Step 3:** Backup current `ghostlink_gui_modern/src/components/WorkersTab.tsx`
- [ ] **Step 4:** Replace with provided `WorkersTab_FIXED.tsx` (rename to `WorkersTab.tsx`)
- [ ] **Step 5:** Backup current `ghostlink_gui_modern/src/App.tsx`
- [ ] **Step 6:** Replace with provided `App_FIXED.tsx` (rename to `App.tsx`)
- [ ] **Step 7:** Create `ghostlink_gui_modern/vite.config.ts` (provided)
- [ ] **Step 8:** Create `ghostlink_gui_modern/.env.example` (provided)
- [ ] **Step 9:** Create `.env` file (copy from `.env.example`)

---

## Testing Checklist

### Test Chat Tab
1. Start backend: `cargo run -p ghost-link -- serve 127.0.0.1 8003`
2. Start frontend: `cd ghostlink_gui_modern && npm run dev`
3. Go to Chat tab
4. Select a model
5. Type message: "Hello, test message"
6. Click Send or press Enter
7. **Verify:** Message appears in chat + backend response comes back
8. **Check browser console:** No CORS errors

### Test Workers Tab
1. Go to Workers tab
2. Observe worker list populated
3. Wait 5 seconds, observe worker load updates
4. Click Power button on a worker
5. **Verify:** Worker disconnects + list refreshes
6. **Check browser console:** No errors

### Test Models Tab
1. Go to Models tab
2. Click "Load Model" on a model
3. **Verify:** Model status changes to "Loaded"
4. Go to Chat tab
5. Model selector should show loaded model
6. **Verify:** Can select and use the model

### Health Check
1. Backend running
2. Frontend running
3. Open browser DevTools → Network tab
4. Observe API calls:
   - `GET /api/models` → 200 OK
   - `POST /api/models/load` → 200 OK
   - `POST /api/inference/chat` → 200 OK
   - `GET /api/workers` → 200 OK

---

## Backend Verification

The backend is **already complete**. Verify it's working:

```bash
# Start backend
cd crates/ghost-link
cargo run -- serve 127.0.0.1 8003

# In another terminal, test endpoints:
curl http://localhost:8003/health
# Expected: {"status":"healthy","version":"0.1.0-alpha.0",...}

curl http://localhost:8003/api/models
# Expected: {"models":[...], "current_model":"..."}

curl -X POST http://localhost:8003/api/models/load \
  -H "Content-Type: application/json" \
  -d '{"model":"meta-llama/Llama-3-8B-Instruct"}'
# Expected: {"status":"ok","current_model":"..."}
```

---

## Production Hardening (Next Phase)

After fixes are deployed and tested:

1. **Error Boundaries:** Add React error boundary wrapper
2. **Retry Logic:** Exponential backoff on failed API calls
3. **Request Cancellation:** Cancel in-flight requests on unmount
4. **Session Persistence:** Save chat history to localStorage
5. **Offline Mode:** Graceful degradation when backend unreachable
6. **Loading Skeleton:** Better UX during initial data load
7. **API Versioning:** Prefix all routes with `/v1/` for future compatibility
8. **CORS Hardening:** Review backend CORS headers in production
9. **Rate Limiting:** Client-side throttling on rapid API calls
10. **Analytics:** Log errors and user actions for debugging

---

## Deployment Commands

```bash
# Development
cd ghostlink_gui_modern
npm install
npm run dev

# Production build
npm run build
# Outputs to: dist/

# Run backend
cd ../crates/ghost-link
GHOSTLINK_CI_RUN=0 cargo run --release -- serve 0.0.0.0 8003
```

---

## Summary of Changes

| File | Change | Type | Impact |
|------|--------|------|--------|
| ChatTab.tsx | Capture input before clearing | Code Fix | HIGH - Chat now works |
| WorkersTab.tsx | Add polling + disconnect handler | Code Fix | HIGH - Workers now update |
| App.tsx | Initialize apiBase on mount | Code Fix | HIGH - Backend discovery |
| vite.config.ts | Add proxy configuration | Config | MEDIUM - CORS bypass |
| .env.example | Environment variable template | Config | LOW - Optional |

**Total changes:** 5 files, ~30 lines of new/modified code  
**Risk level:** LOW - surgical fixes to existing code  
**Deployment:** No database migrations, no backend changes required

---

## Files Provided in This Session

1. ✅ `ACTUAL_DEBUG_ANALYSIS.md` - Detailed component analysis
2. ✅ `ghostlink_gui_modern/src/components/ChatTab.tsx` - Fixed chat component
3. ✅ `ghostlink_gui_modern/src/components/WorkersTab_FIXED.tsx` - Fixed workers component
4. ✅ `ghostlink_gui_modern/src/App_FIXED.tsx` - Fixed app init
5. ✅ `ghostlink_gui_modern/vite.config.ts` - Vite proxy config
6. ✅ `ghostlink_gui_modern/.env.example` - Environment template
7. ✅ `PRODUCTION_FIXES.md` - This file

---

## Next Steps

1. **Apply fixes immediately** (all provided in this session)
2. **Test locally** (15 min) using checklist above
3. **Deploy to staging** (if applicable)
4. **Monitor logs** for any API errors
5. **Plan Phase 2** hardening for production

---

**Status:** ✅ All critical bugs identified and fixed  
**Estimated implementation time:** 15 minutes  
**Risk assessment:** LOW - all changes are isolated and backward-compatible
