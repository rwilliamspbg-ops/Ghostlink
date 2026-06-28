# Mohawk Inference Engine - Professional Dashboard Guide

## 🎯 Overview

The Mohawk Inference Engine GUI provides a comprehensive, LM Studio-style dashboard with enterprise-grade features for managing multi-device inference sessions securely.

---

## 📊 Dashboard Features (LM Studio + Mohawk Unique)

### 1. 📚 Model Library Manager
**Similar to:** LM Studio's model library  
**Mohawk Extensions:** Multi-device layer splitting, PQC-secured workers

#### Features:
- ✅ **Model Browser** - Browse available models with quantization options
- ✅ **Download Models** - Download Safetensors/HF models directly
- ✅ **Upload Local Models** - Load local model files
- ✅ **Quantization Selector** - Q4_K_M, Q5_K_M, Q8_0, FP16 options
- ✅ **Device Split Configuration** - Configure multi-device layer splitting:
  ```
  Format: 'cpu_threads;gpu_ids'
  Example: 'cpu;0,1,2,3;cuda:0,1'
  ```
- ✅ **Model Status Tracking** - Ready/Loading/Failed states

#### Quick Actions:
```python
# Download a model
download_btn.click()

# Upload local model
upload_btn.click()

# Load selected model
load_selected_model()
```

---

### 2. 💬 Chat Interface
**Similar to:** LM Studio's chat panel  
**Mohawk Extensions:** Multi-session support, context management

#### Features:
- ✅ **Conversation History** - Scrollable message history
- ✅ **Parameter Controls**:
  - Temperature (0.0 - 2.0)
  - Top-p sampling
  - Max tokens generation
- ✅ **System Prompt Editor** - Customizable system instructions
- ✅ **Context Management**:
  - Context size tracking
  - Clear history button
  - Token usage monitoring
- ✅ **Multi-turn Conversations** - Full context retention

#### Usage Examples:
```python
# Send a message
message_input.setPlainText("What is quantum computing?")
send_message()

# Get response (automatically appends to chat)
# Response appears with latency metrics
```

---

### 3. 📊 Performance Metrics Dashboard
**Similar to:** LM Studio's stats panel  
**Mohawk Extensions:** Real-time GPU/CPU/Multi-device tracking

#### Features:
- ✅ **Throughput Chart** - Requests per second (real-time)
- ✅ **Latency Monitoring**:
  - p50 latency (median)
  - p95 latency (95th percentile)
  - p99 latency (99th percentile)
- ✅ **Resource Usage Charts**:
  - CPU utilization
  - Memory consumption
  - GPU utilization per device
- ✅ **Statistics Summary**:
  - Total requests processed
  - Average latency
  - Success rate
  - Active sessions count
  - Peak throughput

#### Real-time Updates:
```python
# Metrics update every second automatically
# Dashboard shows live charts and progress bars
```

---

### 4. 🔗 Session Manager
**Similar to:** LM Studio's session queue  
**Mohawk Extensions:** Priority queuing, multi-worker support

#### Features:
- ✅ **Session Table** - View all active sessions with:
  - Session ID
  - Model name
  - Status (Queued/Running/Completed)
  - Throughput metrics
  - Latency percentiles
  - Tokens per second
  - Start time
- ✅ **Queue Configuration**:
  - Max queue size setting
  - Priority levels (High/Normal/Low)
- ✅ **Job Management**:
  - Queue new jobs with priority
  - Cancel running sessions
  - Monitor session lifecycle

#### Quick Actions:
```python
# Queue high-priority job
high_priority_btn.click()

# Queue normal job
normal_priority_btn.click()

# Cancel specific session
cancel_session(session_row)
```

---

### 5. ⚙️ Worker Configuration
**Mohawk Unique Feature:** Multi-device layer splitting management

#### Features:
- ✅ **Worker List** - View all connected workers with:
  - Worker ID
  - Host:Port address
  - Connection status
  - Loaded model
  - GPU thread count
  - Current load percentage
- ✅ **Worker Actions**:
  - Connect/Disconnect workers
  - Restart failed workers
  - Monitor worker health
- ✅ **Multi-device Configuration**:
  - Configure layer splitting across devices
  - Set GPU thread counts per worker
  - Balance load across workers

#### Usage Example:
```python
# Add new worker
add_worker(host="localhost", port=8004)

# Configure device splitting for worker
device_split_config.setText("cpu;0,1,2,3;cuda:0,1")

# Connect to worker
connect_to_worker("localhost", 8003)
```

---

### 6. 🔒 Security Center
**Mohawk Unique Feature:** Comprehensive security dashboard

#### Features:
- ✅ **JWT Authentication Status**:
  - Token validity indicator
  - Expiry time display
  - Refresh token button
- ✅ **mTLS Configuration**:
  - Certificate validity check
  - Key encryption status (Fernet)
  - Certificate path management
- ✅ **Post-Quantum Cryptography (PQC)**:
  - Hybrid KEM support toggle
  - liboqs integration status
  - X25519 key exchange configuration
- ✅ **Security Event Log** - Immutable audit trail

#### Security Features:
```python
# JWT Authentication (RS256)
# Token expiry: 24 hours
# Refresh window: 1 hour

# mTLS with client certificates
# Certificate encryption with Fernet

# PQC Hybrid Mode (Optional)
# X25519 + Kyber for quantum-resistant security
```

---

### 7. 📜 Conversation History
**Similar to:** LM Studio's history panel  
**Mohawk Extensions:** Model usage tracking, analytics

#### Features:
- ✅ **History Table** - View all conversations with:
  - Timestamp
  - Model used
  - Tokens consumed
  - Duration
  - Status (Completed/Failed)
- ✅ **Usage Statistics**:
  - Total tokens processed
  - Average latency across sessions
  - Models usage count
- ✅ **Search & Filter** - Find specific conversations

---

## 🎨 UI Layout

```
┌─────────────────────────────────────────────────────────────┐
│ 🦅 Mohawk Inference Engine v2.1.0                           │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ 📚 Model Library    | 💬 Chat Interface              │  │
│  ├──────────────────────────────────────────────────────┤  │
│  │                                                        │  │
│  │  [Tabs: Models | Chat | Metrics | Sessions           │  │
│  │          | Workers | Security | History]             │  │
│  │                                                        │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ Navigation Toolbar                                    │  │
│  │ [📚 Models] [💬 Chat] [📊 Metrics] ...                │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐  │
│  | Status Bar: "Throughput: 1,250 req/s | Latency..."   │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

---

## 🚀 Quick Start Guide

### Installation

```bash
# Clone repository
cd C:\Users\rwill\Mohawk-Inference-Engine

# Create virtual environment
python -m venv venv
venv\Scripts\activate

# Install dependencies
pip install -r requirements.txt

# Generate authentication key (first run)
mkdir -p certs
python mohawk_gui/main.py --key-file certs/auth_key.pem
```

### Running the Application

```bash
# Development mode
python mohawk_gui/main.py

# Production mode with SSL
python mohawk_gui/main.py \
    --host 0.0.0.0 \
    --port 8003 \
    --ssl-enabled \
    --key-file certs/auth_key.pem
```

### Building Executable

```bash
# Windows build
build_windows.bat

# Or use PyInstaller directly
pyinstaller \
    --name=Mohawk-Inference-Engine \
    --onefile \
    --windowed \
    mohawk_gui/main.py
```

---

## 🎯 Key Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+Shift+M` | Show/Hide Main Window (tray icon) |
| `Shift+Enter` | New line in chat input |
| `Enter` | Send message |
| `F1` | Show help |

---

## 🔧 Configuration Options

### Command Line Arguments

```bash
--host <host>        # Bind address (default: localhost)
--port <port>        # Port number (default: 8003)
--key-file <path>    # Auth key file path
--ssl-enabled        # Enable SSL/TLS
--metrics-interval <ms>  # Update interval (default: 1000ms)
```

### config.toml Settings

```toml
[mohawk]
host = "localhost"
port = 8003
ssl_enabled = false

[workers]
enabled = true
auto_discover = false
timeout_ms = 5000
max_connections = 100

[sessions]
max_concurrent = 10
default_batch_size = 32

[metrics]
sampling_rate = 0.1
buffer_window_size = 1000

[security]
jwt_expiry_hours = 24
refresh_window_hours = 1
audit_enabled = true
```

---

## 📊 Dashboard Screenshots (Feature Map)

### Tab 1: Model Library
- **Top:** Search bar with model type/quant filters
- **Middle:** Model table with status indicators
- **Bottom:** Model details panel (load config)

### Tab 2: Chat Interface
- **Left:** Conversation history scroll area
- **Right:** Settings panel (temp, top-p, max tokens)

### Tab 3: Performance Metrics
- **Top:** Throughput chart (PyQtGraph)
- **Middle:** Latency percentiles display
- **Bottom:** Resource usage bars (CPU/Mem/GPU)

### Tab 4: Session Manager
- **Top:** Active sessions table
- **Bottom:** Queue configuration panel

### Tab 5: Worker Configuration
- **Top:** Workers list with status
- **Bottom:** Add/Configure worker settings

### Tab 6: Security Center
- **Top:** Security status cards (JWT, mTLS, PQC)
- **Bottom:** Security event log

### Tab 7: Conversation History
- **Top:** Usage history table
- **Bottom:** Statistics summary

---

## 🎓 Best Practices

### Model Loading
1. Start with quantized models (Q4_K_M or Q5_K_M) for better memory efficiency
2. Configure device splitting for multi-GPU setups
3. Monitor GPU utilization in Metrics tab

### Chat Usage
1. Set temperature lower (0.5-0.7) for deterministic responses
2. Increase max tokens for longer outputs
3. Use system prompt to customize behavior

### Worker Management
1. Connect workers before loading models
2. Monitor worker load in Workers tab
3. Restart workers if latency increases

### Security Setup
1. Always enable JWT authentication
2. Configure mTLS for production deployments
3. Enable PQC for quantum-resistant security (optional)

---

## 🐛 Troubleshooting

### Common Issues

**Issue:** "Import error: PyQt6"  
**Solution:** `pip install PyQt6 pyqtgraph`

**Issue:** "No models loaded"  
**Solution:** Use Model Library → Download or Upload models

**Issue:** "Worker connection failed"  
**Solution:** Check worker is running on correct port (default: 8003)

**Issue:** "SSL certificate error"  
**Solution:** Generate certificates in `certs/` directory

---

## 📞 Support

For issues and questions:
- Check the [GUI Documentation Index](../GUI_DOCUMENTATION_INDEX.md)
- Review the [Executive Summary](../GUI_EXECUTIVE_SUMMARY.md)
- Open an issue on GitHub

---

**Mohawk Inference Engine v2.1.0 - Production Ready!** 🦅
