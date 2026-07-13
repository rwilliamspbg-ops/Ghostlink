# Ghostlink Commercial Quickstart

## 1. Clone and start
```bash
git clone https://github.com/rwilliamspbg-ops/Ghostlink.git
cd Ghostlink
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
```

## 2. Start the model manager
```bash
python3 model_manager.py
```

## 3. Start the backend
```bash
python3 real_llm_proxy.py backend
```

## 4. Verify model load
```bash
curl -X POST http://127.0.0.1:8001/api/models/load -H 'Content-Type: application/json' -d '{"model":"tinyllama"}'
```

## 5. Optional: run the GUI
```bash
python3 ghostlink_gui.py
```
