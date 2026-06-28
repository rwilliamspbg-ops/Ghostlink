# 🚀 Mohawk Inference Engine - Quick Start Guide

## ⚡ 3-Minute Setup

### Step 1: Install Dependencies (1 minute)

```bash
cd C:\Users\rwill\Mohawk-Inference-Engine

# Create virtual environment
python -m venv venv

# Activate it
venv\Scripts\activate

# Install all dependencies
pip install -r requirements.txt
```

### Step 2: Run the Dashboard (30 seconds)

```bash
# Generate auth key and start
python mohawk_gui/main.py
```

That's it! The dashboard opens automatically with all features ready.

---

## 🎯 First-Time Usage

### 1. Load a Model (Model Library Tab)

1. Click **"📚 Models"** tab
2. Click **"⬇️ Download"** or **"⬆️ Upload"** to get models
3. Select quantization: **Q4_K_M** (best balance of speed/quality)
4. Configure device splitting if using multi-GPU
5. Click **"🚀 Load Model"**

### 2. Start Chatting (Chat Interface Tab)

1. Click **"💬 Chat"** tab
2. Type your message in the input box
3. Adjust settings if needed:
   - Temperature: **0.7** (balanced creativity)
   - Max Tokens: **2048** (good for most tasks)
4. Press **➤ Send** or hit **Enter**

### 3. Monitor Performance (Metrics Tab)

1. Click **"📊 Metrics"** tab
2. Watch real-time throughput and latency
3. Monitor GPU/CPU/Memory usage
4. View conversation statistics

---

## 🎨 Dashboard Tour

### 📚 Model Library Tab
- **Browse models** with search and filters
- **Download/Upload** new models
- **Configure quantization** and device splitting
- **Load models** into inference engine

### 💬 Chat Interface Tab
- **Type messages** in the input box
- **Adjust generation parameters** (temp, top-p, max tokens)
- **View conversation history** with scroll
- **Clear history** when needed

### 📊 Performance Metrics Tab
- **Real-time throughput** charts
- **Latency percentiles** (p50/p95/p99)
- **Resource usage** monitoring (CPU/Mem/GPU)
- **Statistics summary** with totals

### 🔗 Session Manager Tab
- **View active sessions** in table
- **Queue new jobs** with priority
- **Monitor throughput** and latency per session
- **Cancel sessions** when done

### ⚙️ Worker Configuration Tab
- **Manage workers** (connect/disconnect)
- **Configure device splitting** for multi-GPU
- **Monitor worker load** and status
- **Restart failed workers**

### 🔒 Security Center Tab
- **JWT Authentication** status
- **mTLS configuration** for secure connections
- **PQC support** toggle (optional quantum resistance)
- **Security event logs**

### 📜 Conversation History Tab
- **Browse all conversations** with timestamps
- **View tokens used** and duration
- **Statistics** on total usage

---

## 💡 Pro Tips

### For Best Performance:
1. **Use Q4_K_M quantization** - Best balance of speed/quality
2. **Configure device splitting** for multi-GPU setups
3. **Monitor latency** in Metrics tab to tune parameters
4. **Keep temperature low (0.5-0.7)** for consistent responses

### For Security:
1. **Enable JWT authentication** (always on by default)
2. **Configure mTLS** for production deployments
3. **Review security logs** regularly

### For Multi-Device Inference:
1. Go to **Workers tab** and add multiple workers
2. Configure **device splitting** in Model Library
3. Monitor **load balancing** in Metrics tab

---

## 🔧 Common Commands

```bash
# Start with custom port
python mohawk_gui/main.py --port 9003

# Enable SSL
python mohawk_gui/main.py --ssl-enabled --key-file certs/auth_key.pem

# Build executable (Windows)
build_windows.bat

# Run tests
pytest mohawk_gui/ -v
```

---

## 📊 What You Get

✅ **LM Studio-style Model Library** with quantization options  
✅ **Real-time Chat Interface** with context management  
✅ **Performance Dashboard** with live charts  
✅ **Session Manager** for multi-task inference  
✅ **Worker Configuration** with multi-device splitting  
✅ **Security Center** with PQC + mTLS support  
✅ **Conversation History** with usage analytics  

---

## 🆘 Need Help?

- **Full Documentation:** See `DASHBOARD_FEATURES.md`
- **Architecture:** See `GUI_IMPLEMENTATION_PLAN.md`
- **Production Readiness:** See `GUI_PRODUCTION_READINESS.md`
- **Issues:** Check GitHub repository

---

**Ready to start?** Just run `python mohawk_gui/main.py` and enjoy! 🦅
