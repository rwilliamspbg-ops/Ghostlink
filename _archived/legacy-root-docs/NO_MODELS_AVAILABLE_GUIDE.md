# 🔧 No Models Available - Troubleshooting & Solutions

## Problem
Chat tab shows "No models available - Go to Models tab"

## Solutions

### 1. Check if Backend is Running ✅
```bash
# Test backend connectivity
curl http://127.0.0.1:8003/health

# Should return:
# {"status": "ok", "uptime_s": 123, "current_model": "none"}
```

### 2. Check if Backend Has Models ✅
```bash
# List available models
curl http://127.0.0.1:8003/api/models

# Should return:
# {"models": [...], "current_model": "none"}
```

### 3. If Backend Returns Empty Models

**Option A: Download Models via GUI**
1. Go to **Models tab** → **Hugging Face**
2. Search for a model (e.g., "Mistral" or "Llama")
3. Click "Download"
4. Wait for download to complete
5. Go back to **Chat tab** → Model dropdown should now show models

**Option B: Download Models via Command Line**
```bash
# Example: Download Mistral via backend
curl -X POST http://127.0.0.1:8003/api/models/download \
  -H "Content-Type: application/json" \
  -d '{"model_id": "mistralai/Mistral-7B-v0.1"}'
```

**Option C: Manually Add Test Models**
If you're testing, you can add mock data to the backend response.

### 4. Verify Models Tab Displays Models

1. Click **Models** tab
2. You should see a table with available models
3. If empty, download a model first (see Option A)

### 5. After Models are Downloaded

1. Go to **Chat** tab
2. Model dropdown should now populate
3. Select a model
4. Type a message and send

---

## Common Issues

### Issue: Models Downloaded but Not Showing in Chat

**Solution:**
```
1. Refresh browser (Ctrl+R or Cmd+R)
2. Go to Models tab → Click "Refresh"
3. Go back to Chat tab
4. Dropdown should update
```

### Issue: Backend Returns Error on /api/models

**Solution:**
```
1. Check backend logs for errors
2. Verify backend is fully started
3. Try restarting backend
4. Ensure backend version is compatible
```

### Issue: Model Download Stuck

**Solution:**
```
1. Cancel the operation
2. Check internet connection
3. Try downloading a smaller model first
4. Check disk space available
```

### Issue: Port 8003 Connection Refused

**Solution:**
```
1. Start the backend server
2. Verify backend is listening on 8003
3. Check firewall settings
4. If using remote backend, update vite.config.ts:
   target: 'http://your-backend:8003'
```

---

## Complete Setup Workflow

### Step 1: Ensure Backend is Running
```bash
# Start your backend server
# (command depends on your backend setup)
./ghostlink serve
# or
ghostlink gui  # if backend auto-starts with GUI
```

### Step 2: Verify Backend Health
```bash
curl http://127.0.0.1:8003/health
# Should return success
```

### Step 3: Download a Model
```bash
# Via GUI:
1. Open http://localhost:3000
2. Click Models tab
3. Go to Hugging Face section
4. Search and download a model
5. Wait for completion

# Or via curl:
curl -X POST http://127.0.0.1:8003/api/models/download \
  -H "Content-Type: application/json" \
  -d '{"model_id": "mistralai/Mistral-7B"}'
```

### Step 4: Verify Models are Available
```bash
curl http://127.0.0.1:8003/api/models
# Should return list of models
```

### Step 5: Use Chat Tab
1. Open Chat tab
2. Select model from dropdown
3. Type message
4. Click Send

---

## Example Response Format

### /api/models Response
```json
{
  "models": [
    {
      "name": "mistral-7b",
      "size_gb": 7.5,
      "type": "chat",
      "quantization": "Q4",
      "status": "ready",
      "usable": true
    },
    {
      "name": "llama-2-7b",
      "size_gb": 7.2,
      "type": "text-generation",
      "quantization": "Q4",
      "status": "ready",
      "usable": true
    }
  ],
  "current_model": "mistral-7b"
}
```

---

## Backend Integration

### What the GUI Expects

**Models with these properties:**
```typescript
{
  name: string;              // Model identifier
  size_gb: number;           // Size in GB
  type: string;              // "chat", "text-generation", etc.
  quantization: string;      // "Q4", "Q5", "F16", etc.
  status: string;            // "ready", "loading", "error"
  usable?: boolean;          // Only show if true and status="ready"
}
```

**Only models with:**
- `status === "ready"`
- `type` includes: "chat", "text-generation", or similar
- Will show in Chat tab dropdown

---

## Testing Without Backend

If you don't have a real backend running:

1. The GUI will show "No models available"
2. Use the **Models** tab to simulate downloading
3. The API client has fallback data for HuggingFace search
4. To test Chat, connect to a real backend

---

## Still Having Issues?

### Debug Steps

1. **Check browser console** (F12)
   - Look for errors in the Console tab
   - Check Network tab for API calls

2. **Check backend logs**
   - Look for `/api/models` endpoint calls
   - Check for connection refused errors

3. **Verify API connectivity**
   ```bash
   # From your machine
   curl -v http://127.0.0.1:8003/api/models
   ```

4. **Check if models exist locally**
   ```bash
   # Depends on your backend setup
   # Usually in ~/.ghostlink/models or similar
   ```

5. **Restart everything**
   ```bash
   # Stop GUI
   Ctrl+C in terminal
   
   # Stop backend
   Ctrl+C or kill process
   
   # Restart backend
   # Start backend first, verify it's running
   
   # Restart GUI
   npm run dev
   ```

---

## Need More Help?

- Check backend logs/error messages
- Verify backend version compatibility
- Ensure models are actually downloaded to backend
- Test `/api/models` endpoint directly with curl
- Monitor network traffic in browser DevTools

---

**Quick Checklist:**
- [ ] Backend running (curl health check)
- [ ] Models downloaded (curl /api/models)
- [ ] Browser refreshed (Ctrl+R)
- [ ] Chat tab loaded
- [ ] Model dropdown populated
- [ ] Model selected
- [ ] Message typed
- [ ] Send clicked

Once all checkboxes are done, chat should work! ✅
