#!/usr/bin/env python3
"""Ghostlink Studio Tkinter GUI.

This frontend targets the current Ghostlink API surface exposed by the Rust
backend. Backend orchestration stays in the Rust ``gui`` command.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import threading
import time
import tkinter as tk
from tkinter import messagebox, scrolledtext, simpledialog, ttk
from urllib.parse import urlparse

import requests

try:
    from huggingface_hub import HfApi
except ImportError:  # pragma: no cover
    HfApi = None


DEFAULT_API_BASE = "http://127.0.0.1:8003"


class GhostlinkGUI:
    def __init__(self, root: tk.Tk, api_base: str | None = None):
        self.root = root
        self.root.title("Ghostlink Studio")
        self.root.geometry("1280x860")
        self.root.minsize(1100, 720)

        self.http = requests.Session()
        self.request_timeout = (1.0, 3.0)
        self.backend_online = None
        self.last_ping = "never"
        self.hf_api = HfApi() if HfApi is not None else None

        configured_base = api_base or os.getenv("GHOSTLINK_GUI_BASE_URL") or DEFAULT_API_BASE
        self.api_base = self.normalize_base_url(configured_base)
        parsed = urlparse(self.api_base)
        self.backend_host = parsed.hostname or "127.0.0.1"
        self.backend_port = parsed.port or 8003
        self.allow_backend_autostart = self.backend_host in {"127.0.0.1", "localhost"}

        self.configure_style()
        self.build_shell()
        self.build_sidebar()
        self.build_content()
        self.build_tabs()

        self.root.protocol("WM_DELETE_WINDOW", self.on_close)

        self.refresh_health()
        self.schedule_refreshes()

    def normalize_base_url(self, value: str) -> str:
        candidate = (value or "").strip().rstrip("/")
        if not candidate:
            candidate = DEFAULT_API_BASE
        if "://" not in candidate:
            candidate = f"http://{candidate}"
        parsed = urlparse(candidate)
        if not parsed.scheme or not parsed.netloc:
            return DEFAULT_API_BASE
        return f"{parsed.scheme}://{parsed.netloc}"

    def configure_style(self) -> None:
        self.root.configure(bg="#111318")
        style = ttk.Style()
        try:
            style.theme_use("clam")
        except tk.TclError:
            pass
        style.configure("TNotebook", background="#111318", borderwidth=0)
        style.configure("TNotebook.Tab", padding=(14, 8), background="#1c1f26", foreground="#e5e7eb")
        style.map("TNotebook.Tab", background=[("selected", "#2a2f3a")], foreground=[("selected", "#ffffff")])
        style.configure("Treeview", background="#151922", fieldbackground="#151922", foreground="#e5e7eb", borderwidth=0)
        style.configure("Treeview.Heading", background="#202531", foreground="#ffffff", relief="flat")
        style.configure("TButton", padding=(10, 6))
        style.configure("TLabel", background="#111318", foreground="#e5e7eb")

    def build_shell(self) -> None:
        self.shell = tk.Frame(self.root, bg="#111318")
        self.shell.pack(fill="both", expand=True)

    def build_sidebar(self) -> None:
        sidebar = tk.Frame(self.shell, width=230, bg="#0b0d12", highlightthickness=1, highlightbackground="#232733")
        sidebar.pack(side="left", fill="y")
        sidebar.pack_propagate(False)

        title = tk.Label(sidebar, text="GHOSTLINK", bg="#0b0d12", fg="#6ea8ff", font=("Arial", 18, "bold"))
        title.pack(anchor="w", padx=16, pady=(18, 4))

        subtitle = tk.Label(sidebar, text="Studio GUI", bg="#0b0d12", fg="#9ca3af", font=("Arial", 10))
        subtitle.pack(anchor="w", padx=16, pady=(0, 16))

        self.health_indicator = tk.Label(sidebar, text="● checking", bg="#0b0d12", fg="#f59e0b", font=("Arial", 11, "bold"))
        self.health_indicator.pack(anchor="w", padx=16, pady=(0, 12))

        self.health_summary = tk.Label(sidebar, text=self.api_base, wraplength=190, justify="left", bg="#0b0d12", fg="#d1d5db", font=("Arial", 9))
        self.health_summary.pack(anchor="w", padx=16, pady=(0, 18))

        self.active_model_label = tk.Label(sidebar, text="Model: loading...", wraplength=190, justify="left", bg="#0b0d12", fg="#d1d5db", font=("Arial", 10, "bold"))
        self.active_model_label.pack(anchor="w", padx=16, pady=(0, 18))

        self.sidebar_buttons: list[tk.Button] = []
        for index, text in enumerate(["Chat", "Models", "Metrics", "Sessions", "Workers", "Security"]):
            button = tk.Button(sidebar, text=text, command=lambda idx=index: self.switch_tab(idx), relief="flat", bg="#151922", fg="#e5e7eb", activebackground="#2b3442", activeforeground="#ffffff", padx=14, pady=10)
            button.pack(fill="x", padx=12, pady=4)
            self.sidebar_buttons.append(button)

    def build_content(self) -> None:
        content = tk.Frame(self.shell, bg="#111318")
        content.pack(side="left", fill="both", expand=True)

        header = tk.Frame(content, bg="#111318")
        header.pack(fill="x", padx=20, pady=(18, 10))

        self.header_title = tk.Label(header, text="Ghostlink Studio", bg="#111318", fg="#ffffff", font=("Arial", 22, "bold"))
        self.header_title.pack(anchor="w")

        self.header_status = tk.Label(header, text="Starting...", bg="#111318", fg="#9ca3af", font=("Arial", 10))
        self.header_status.pack(anchor="w", pady=(4, 0))

        self.notebook = ttk.Notebook(content)
        self.notebook.pack(fill="both", expand=True, padx=16, pady=(0, 16))

    def build_tabs(self) -> None:
        self.chat_tab = tk.Frame(self.notebook, bg="#111318")
        self.models_tab = tk.Frame(self.notebook, bg="#111318")
        self.metrics_tab = tk.Frame(self.notebook, bg="#111318")
        self.sessions_tab = tk.Frame(self.notebook, bg="#111318")
        self.workers_tab = tk.Frame(self.notebook, bg="#111318")
        self.security_tab = tk.Frame(self.notebook, bg="#111318")

        for tab, label in [
            (self.chat_tab, "Chat"),
            (self.models_tab, "Models"),
            (self.metrics_tab, "Metrics"),
            (self.sessions_tab, "Sessions"),
            (self.workers_tab, "Workers"),
            (self.security_tab, "Security"),
        ]:
            self.notebook.add(tab, text=label)

        self.build_chat_tab()
        self.build_models_tab()
        self.build_metrics_tab()
        self.build_sessions_tab()
        self.build_workers_tab()
        self.build_security_tab()

    def switch_tab(self, index: int) -> None:
        self.notebook.select(index)

    def on_close(self) -> None:
        self.root.destroy()

    def is_backend_reachable(self) -> bool:
        try:
            response = self.http.get(f"{self.api_base}/health", timeout=1.5)
            return response.status_code == 200
        except Exception:
            return False

    def api_call(self, endpoint: str, method: str = "GET", payload: dict | None = None) -> dict:
        if endpoint != "/health" and not self.is_backend_reachable():
            return {"error": "Backend offline"}

        url = f"{self.api_base}{endpoint}"
        try:
            if method == "GET":
                response = self.http.get(url, timeout=self.request_timeout)
            elif method == "POST":
                response = self.http.post(url, json=payload, timeout=self.request_timeout)
            else:
                return {"error": f"Unsupported HTTP method: {method}"}

            if response.status_code not in {200, 201}:
                try:
                    detail = response.json().get("error") or response.json().get("detail")
                except Exception:
                    detail = response.text.strip()
                return {"error": detail or f"HTTP {response.status_code}"}

            try:
                return response.json()
            except ValueError:
                return {"status": "ok"}
        except Exception as exc:
            return {"error": str(exc)}

    def schedule_refreshes(self) -> None:
        self.root.after(3000, self.poll_health)
        self.root.after(5000, self.refresh_models)
        self.root.after(5000, self.refresh_metrics)
        self.root.after(5000, self.refresh_sessions)
        self.root.after(5000, self.refresh_workers)

    def poll_health(self) -> None:
        result = self.api_call("/health")
        if result.get("error"):
            self.backend_online = False
            self.last_ping = "offline"
            self.health_indicator.configure(text="● offline", fg="#ef4444")
            self.health_summary.configure(text=result["error"])
            self.header_status.configure(text=f"Disconnected: {result['error']}")
        else:
            self.backend_online = True
            self.last_ping = time.strftime("%H:%M:%S")
            self.health_indicator.configure(text="● online", fg="#10b981")
            self.health_summary.configure(text=f"{self.api_base}\nLast ping: {self.last_ping}")
            current_model = result.get("current_model") or "none"
            self.active_model_label.configure(text=f"Model: {current_model}")
            self.header_status.configure(text=f"Backend healthy. Uptime {result.get('uptime_s', '?')}s")
        self.root.after(3000, self.poll_health)

    def build_chat_tab(self) -> None:
        container = tk.Frame(self.chat_tab, bg="#111318")
        container.pack(fill="both", expand=True, padx=8, pady=8)

        left = tk.Frame(container, bg="#111318")
        left.pack(side="left", fill="both", expand=True, padx=(0, 12))

        tk.Label(left, text="Prompt", bg="#111318", fg="#ffffff", font=("Arial", 12, "bold")).pack(anchor="w")
        self.chat_prompt = scrolledtext.ScrolledText(left, height=8, bg="#151922", fg="#e5e7eb", insertbackground="#ffffff", wrap="word")
        self.chat_prompt.pack(fill="x", pady=(6, 10))

        controls = tk.Frame(left, bg="#111318")
        controls.pack(fill="x", pady=(0, 10))

        self.temperature_var = tk.DoubleVar(value=0.7)
        self.top_p_var = tk.DoubleVar(value=0.9)
        self.top_k_var = tk.IntVar(value=40)
        self.penalty_var = tk.DoubleVar(value=1.1)
        self.max_tokens_var = tk.IntVar(value=256)

        for row, (label, widget) in enumerate([
            ("Temp", ttk.Spinbox(controls, from_=0.0, to=2.0, increment=0.1, textvariable=self.temperature_var, width=8)),
            ("Top P", ttk.Spinbox(controls, from_=0.0, to=1.0, increment=0.05, textvariable=self.top_p_var, width=8)),
            ("Top K", ttk.Spinbox(controls, from_=0, to=100, textvariable=self.top_k_var, width=8)),
            ("Penalty", ttk.Spinbox(controls, from_=1.0, to=2.0, increment=0.05, textvariable=self.penalty_var, width=8)),
            ("Max Tokens", ttk.Spinbox(controls, from_=1, to=32768, textvariable=self.max_tokens_var, width=10)),
        ]):
            tk.Label(controls, text=label, bg="#111318", fg="#d1d5db").grid(row=0, column=row * 2, sticky="w", padx=(0, 4))
            widget.grid(row=0, column=row * 2 + 1, padx=(0, 10))

        sys_frame = tk.LabelFrame(left, text="System Prompt", bg="#111318", fg="#ffffff", padx=8, pady=8)
        sys_frame.pack(fill="both", expand=False, pady=(0, 10))
        self.system_prompt = scrolledtext.ScrolledText(sys_frame, height=4, bg="#151922", fg="#e5e7eb", insertbackground="#ffffff", wrap="word")
        self.system_prompt.insert("1.0", "You are a highly capable AI assistant running on Ghostlink Fabric.")
        self.system_prompt.pack(fill="both", expand=True)

        tools_frame = tk.LabelFrame(left, text="Tool Access (MCP JSON)", bg="#111318", fg="#ffffff", padx=8, pady=8)
        tools_frame.pack(fill="both", expand=False, pady=(0, 10))
        self.tool_access_input = scrolledtext.ScrolledText(tools_frame, height=5, bg="#151922", fg="#e5e7eb", insertbackground="#ffffff", wrap="word")
        self.tool_access_input.insert("1.0", '{"tools": []}')
        self.tool_access_input.pack(fill="both", expand=True)

        button_row = tk.Frame(left, bg="#111318")
        button_row.pack(fill="x")
        tk.Button(button_row, text="Send", command=self.send_message, bg="#3b82f6", fg="#ffffff", relief="flat", padx=16, pady=8).pack(side="left")
        tk.Button(button_row, text="Refresh Health", command=self.refresh_health, bg="#1f2937", fg="#ffffff", relief="flat", padx=16, pady=8).pack(side="left", padx=8)

        right = tk.Frame(container, bg="#111318")
        right.pack(side="left", fill="both", expand=True)
        tk.Label(right, text="Response", bg="#111318", fg="#ffffff", font=("Arial", 12, "bold")).pack(anchor="w")
        self.chat_response = scrolledtext.ScrolledText(right, bg="#151922", fg="#e5e7eb", insertbackground="#ffffff", wrap="word")
        self.chat_response.pack(fill="both", expand=True, pady=(6, 0))

    def build_models_tab(self) -> None:
        top = tk.Frame(self.models_tab, bg="#111318")
        top.pack(fill="x", padx=8, pady=8)

        self.model_filter_var = tk.StringVar(value="")
        tk.Label(top, text="Filter", bg="#111318", fg="#d1d5db").pack(side="left")
        tk.Entry(top, textvariable=self.model_filter_var, bg="#151922", fg="#ffffff", insertbackground="#ffffff", width=28).pack(side="left", padx=8)
        tk.Button(top, text="Refresh", command=self.refresh_models, bg="#1f2937", fg="#ffffff", relief="flat", padx=12, pady=6).pack(side="left", padx=4)
        tk.Button(top, text="Download", command=self.download_model, bg="#1f2937", fg="#ffffff", relief="flat", padx=12, pady=6).pack(side="left", padx=4)
        tk.Button(top, text="Load Selected", command=self.load_selected_model, bg="#3b82f6", fg="#ffffff", relief="flat", padx=12, pady=6).pack(side="left", padx=4)

        columns = ("name", "size", "type", "quant", "status")
        self.models_tree = ttk.Treeview(self.models_tab, columns=columns, show="headings", height=16)
        for column, heading, width in [
            ("name", "Name", 250),
            ("size", "Size (GB)", 90),
            ("type", "Type", 90),
            ("quant", "Quant", 110),
            ("status", "Status", 110),
        ]:
            self.models_tree.heading(column, text=heading)
            self.models_tree.column(column, width=width, anchor="w")
        self.models_tree.pack(fill="both", expand=True, padx=8, pady=(0, 8))

        self.selected_model_var = tk.StringVar(value="Selected model: none")
        tk.Label(self.models_tab, textvariable=self.selected_model_var, bg="#111318", fg="#d1d5db").pack(anchor="w", padx=10, pady=(0, 8))

        self.models_tree.bind("<<TreeviewSelect>>", self.on_model_select)
        self.selected_model_name: str | None = None

        hf_box = tk.LabelFrame(self.models_tab, text="Hugging Face Search", bg="#111318", fg="#ffffff", padx=8, pady=8)
        hf_box.pack(fill="both", expand=False, padx=8, pady=(0, 8))

        hf_top = tk.Frame(hf_box, bg="#111318")
        hf_top.pack(fill="x")
        self.hf_search_var = tk.StringVar(value="")
        tk.Entry(hf_top, textvariable=self.hf_search_var, bg="#151922", fg="#ffffff", insertbackground="#ffffff", width=34).pack(side="left", padx=(0, 8))
        tk.Button(hf_top, text="Search HF", command=self.search_hf_models, bg="#1f2937", fg="#ffffff", relief="flat", padx=12, pady=6).pack(side="left", padx=4)
        tk.Button(hf_top, text="Download Selected HF", command=self.download_selected_hf_model, bg="#3b82f6", fg="#ffffff", relief="flat", padx=12, pady=6).pack(side="left", padx=4)
        tk.Button(hf_top, text="Download + Load", command=self.download_and_load_selected_hf_model, bg="#0f766e", fg="#ffffff", relief="flat", padx=12, pady=6).pack(side="left", padx=4)

        self.hf_models_tree = ttk.Treeview(hf_box, columns=("repo_id", "likes", "downloads", "pipeline"), show="headings", height=9)
        for column, heading, width in [
            ("repo_id", "Repo ID", 320),
            ("likes", "Likes", 70),
            ("downloads", "Downloads", 90),
            ("pipeline", "Pipeline", 120),
        ]:
            self.hf_models_tree.heading(column, text=heading)
            self.hf_models_tree.column(column, width=width, anchor="w")
        self.hf_models_tree.pack(fill="both", expand=True, pady=(8, 0))
        self.hf_models_tree.bind("<<TreeviewSelect>>", self.on_hf_model_select)
        self.selected_hf_model_name: str | None = None

        self.refresh_models()

    def build_metrics_tab(self) -> None:
        frame = tk.Frame(self.metrics_tab, bg="#111318")
        frame.pack(fill="both", expand=True, padx=8, pady=8)

        self.metric_vars = {
            "throughput": tk.StringVar(value="0"),
            "cpu": tk.StringVar(value="0%"),
            "memory": tk.StringVar(value="0%"),
            "gpu": tk.StringVar(value="0%"),
            "latency_p50": tk.StringVar(value="0 ms"),
            "latency_p95": tk.StringVar(value="0 ms"),
        }

        grid = tk.Frame(frame, bg="#111318")
        grid.pack(anchor="nw")
        for row, (label, key) in enumerate([
            ("Throughput", "throughput"),
            ("CPU", "cpu"),
            ("Memory", "memory"),
            ("GPU", "gpu"),
            ("Latency p50", "latency_p50"),
            ("Latency p95", "latency_p95"),
        ]):
            tk.Label(grid, text=label, bg="#111318", fg="#ffffff", font=("Arial", 11, "bold")).grid(row=row, column=0, sticky="w", pady=4, padx=(0, 12))
            tk.Label(grid, textvariable=self.metric_vars[key], bg="#111318", fg="#d1d5db").grid(row=row, column=1, sticky="w", pady=4)

        self.metrics_log = scrolledtext.ScrolledText(frame, height=12, bg="#151922", fg="#e5e7eb", insertbackground="#ffffff")
        self.metrics_log.pack(fill="both", expand=True, pady=(14, 0))

    def build_sessions_tab(self) -> None:
        top = tk.Frame(self.sessions_tab, bg="#111318")
        top.pack(fill="x", padx=8, pady=8)
        tk.Button(top, text="Refresh", command=self.refresh_sessions, bg="#1f2937", fg="#ffffff", relief="flat", padx=12, pady=6).pack(side="left")

        self.sessions_tree = ttk.Treeview(self.sessions_tab, columns=("id", "model", "status", "throughput", "latency", "tokens"), show="headings", height=16)
        for column, heading, width in [
            ("id", "ID", 150),
            ("model", "Model", 220),
            ("status", "Status", 100),
            ("throughput", "Throughput", 110),
            ("latency", "Latency", 90),
            ("tokens", "Tokens", 90),
        ]:
            self.sessions_tree.heading(column, text=heading)
            self.sessions_tree.column(column, width=width, anchor="w")
        self.sessions_tree.pack(fill="both", expand=True, padx=8, pady=(0, 8))

        self.cancel_session_button = tk.Button(self.sessions_tab, text="Cancel Selected Session", command=self.cancel_selected_session, bg="#7f1d1d", fg="#ffffff", relief="flat", padx=12, pady=6)
        self.cancel_session_button.pack(anchor="w", padx=8, pady=(0, 8))

    def build_workers_tab(self) -> None:
        top = tk.Frame(self.workers_tab, bg="#111318")
        top.pack(fill="x", padx=8, pady=8)
        self.worker_host_var = tk.StringVar(value="127.0.0.1")
        self.worker_port_var = tk.StringVar(value="8004")
        tk.Label(top, text="Host", bg="#111318", fg="#d1d5db").pack(side="left")
        tk.Entry(top, textvariable=self.worker_host_var, bg="#151922", fg="#ffffff", insertbackground="#ffffff", width=18).pack(side="left", padx=6)
        tk.Label(top, text="Port", bg="#111318", fg="#d1d5db").pack(side="left")
        tk.Entry(top, textvariable=self.worker_port_var, bg="#151922", fg="#ffffff", insertbackground="#ffffff", width=8).pack(side="left", padx=6)
        tk.Button(top, text="Add Worker", command=self.add_worker, bg="#1f2937", fg="#ffffff", relief="flat", padx=12, pady=6).pack(side="left", padx=4)
        tk.Button(top, text="Connect Workers", command=self.connect_workers, bg="#3b82f6", fg="#ffffff", relief="flat", padx=12, pady=6).pack(side="left", padx=4)
        tk.Button(top, text="Refresh", command=self.refresh_workers, bg="#1f2937", fg="#ffffff", relief="flat", padx=12, pady=6).pack(side="left", padx=4)

        self.workers_tree = ttk.Treeview(self.workers_tab, columns=("id", "host", "port", "status", "model", "threads", "load"), show="headings", height=16)
        for column, heading, width in [
            ("id", "ID", 150),
            ("host", "Host", 150),
            ("port", "Port", 70),
            ("status", "Status", 110),
            ("model", "Model", 220),
            ("threads", "Threads", 90),
            ("load", "Load", 90),
        ]:
            self.workers_tree.heading(column, text=heading)
            self.workers_tree.column(column, width=width, anchor="w")
        self.workers_tree.pack(fill="both", expand=True, padx=8, pady=(0, 8))

    def build_security_tab(self) -> None:
        frame = tk.Frame(self.security_tab, bg="#111318")
        frame.pack(fill="both", expand=True, padx=8, pady=8)
        tk.Label(frame, text="Security Controls", bg="#111318", fg="#ffffff", font=("Arial", 12, "bold")).pack(anchor="w")

        button_row = tk.Frame(frame, bg="#111318")
        button_row.pack(anchor="w", pady=(8, 12))
        tk.Button(button_row, text="Refresh JWT", command=self.refresh_jwt, bg="#1f2937", fg="#ffffff", relief="flat", padx=12, pady=6).pack(side="left", padx=(0, 6))
        tk.Button(button_row, text="Enable PQC", command=self.enable_pqc, bg="#1f2937", fg="#ffffff", relief="flat", padx=12, pady=6).pack(side="left")

        self.security_log = scrolledtext.ScrolledText(frame, height=14, bg="#151922", fg="#e5e7eb", insertbackground="#ffffff")
        self.security_log.pack(fill="both", expand=True)

    def on_model_select(self, _event=None) -> None:
        selection = self.models_tree.selection()
        if not selection:
            self.selected_model_name = None
            self.selected_model_var.set("Selected model: none")
            return
        item = self.models_tree.item(selection[0])
        values = item.get("values", [])
        self.selected_model_name = values[0] if values else None
        self.selected_model_var.set(f"Selected model: {self.selected_model_name or 'none'}")

    def refresh_health(self) -> None:
        def task() -> None:
            result = self.api_call("/health")
            self.root.after(0, lambda: self.update_health_ui(result))

        threading.Thread(target=task, daemon=True).start()

    def update_health_ui(self, result: dict) -> None:
        if result.get("error"):
            self.backend_online = False
            self.health_indicator.configure(text="● offline", fg="#ef4444")
            self.header_status.configure(text=f"Disconnected: {result['error']}")
            self.health_summary.configure(text=result["error"])
            return

        self.backend_online = True
        self.health_indicator.configure(text="● online", fg="#10b981")
        self.header_status.configure(text=f"Backend healthy. Uptime {result.get('uptime_s', '?')}s")
        self.health_summary.configure(text=f"{self.api_base}\nLast ping: {time.strftime('%H:%M:%S')}")
        current_model = result.get("current_model") or "none"
        self.active_model_label.configure(text=f"Model: {current_model}")

    def refresh_models(self) -> None:
        def task() -> None:
            result = self.api_call("/api/models")
            self.root.after(0, lambda: self.update_models_ui(result))

        threading.Thread(target=task, daemon=True).start()

    def update_models_ui(self, result: dict) -> None:
        if result.get("error"):
            self.header_status.configure(text=f"Model refresh failed: {result['error']}")
            return

        for item in self.models_tree.get_children():
            self.models_tree.delete(item)

        models = result.get("models", [])
        needle = self.model_filter_var.get().strip().lower()
        for model in models:
            name = model.get("name", "unknown")
            if needle and needle not in name.lower():
                continue
            self.models_tree.insert("", "end", values=(
                name,
                f"{model.get('size_gb', 0):.1f}",
                model.get("type", "LLM"),
                model.get("quantization", "Unknown"),
                model.get("status", "Unknown"),
            ))

        current_model = result.get("current_model") or "none"
        self.selected_model_var.set(f"Selected model: {self.selected_model_name or current_model}")
        self.active_model_label.configure(text=f"Model: {current_model}")

    def load_selected_model(self) -> None:
        model_name = self.selected_model_name
        if not model_name:
            messagebox.showwarning("Load Model", "Select a model first.")
            return

        def task() -> None:
            result = self.api_call("/api/models/load", "POST", {"model": model_name})
            self.root.after(0, lambda: self.handle_simple_result(result, f"Loaded model: {model_name}"))

        threading.Thread(target=task, daemon=True).start()

    def download_model(self) -> None:
        model_id = simpledialog.askstring("Download Model", "Enter model ID:", parent=self.root)
        if not model_id:
            return

        def task() -> None:
            result = self.api_call("/api/models/download", "POST", {"model_id": model_id.strip()})
            self.root.after(0, lambda: self.handle_simple_result(result, f"Downloaded model: {model_id.strip()}"))

        threading.Thread(target=task, daemon=True).start()

    def on_hf_model_select(self, _event=None) -> None:
        selection = self.hf_models_tree.selection()
        if not selection:
            self.selected_hf_model_name = None
            return
        item = self.hf_models_tree.item(selection[0])
        values = item.get("values", [])
        self.selected_hf_model_name = values[0] if values else None

    def search_hf_models(self) -> None:
        if self.hf_api is None:
            messagebox.showerror("Hugging Face", "huggingface_hub is not installed in this environment.")
            return

        query = self.hf_search_var.get().strip()
        if not query:
            messagebox.showwarning("Hugging Face", "Enter a model search query.")
            return

        def task() -> None:
            try:
                results = list(self.hf_api.list_models(search=query, limit=25))
            except Exception as exc:
                self.root.after(0, lambda: messagebox.showerror("Hugging Face", str(exc)))
                return
            self.root.after(0, lambda: self.update_hf_search_results(results))

        threading.Thread(target=task, daemon=True).start()

    def update_hf_search_results(self, results: list) -> None:
        for item in self.hf_models_tree.get_children():
            self.hf_models_tree.delete(item)

        for model in results:
            repo_id = getattr(model, "modelId", None) or getattr(model, "id", None) or "unknown"
            likes = getattr(model, "likes", 0) or 0
            downloads = getattr(model, "downloads", 0) or 0
            pipeline = getattr(model, "pipeline_tag", None) or "unknown"
            self.hf_models_tree.insert("", "end", values=(repo_id, likes, downloads, pipeline))

        self.header_status.configure(text=f"Hugging Face search returned {len(results)} results")

    def download_selected_hf_model(self) -> None:
        if not self.selected_hf_model_name:
            messagebox.showwarning("Hugging Face", "Select a Hugging Face model first.")
            return
        self._download_hf_model(self.selected_hf_model_name, auto_load=False)

    def download_and_load_selected_hf_model(self) -> None:
        if not self.selected_hf_model_name:
            messagebox.showwarning("Hugging Face", "Select a Hugging Face model first.")
            return
        self._download_hf_model(self.selected_hf_model_name, auto_load=True)

    def _download_hf_model(self, model_id: str, auto_load: bool) -> None:
        def task() -> None:
            result = self.api_call("/api/models/download", "POST", {"model_id": model_id})

            def finalize() -> None:
                if result.get("error"):
                    messagebox.showerror("Hugging Face", result["error"])
                    return
                self.header_status.configure(text=f"Downloaded Hugging Face model: {model_id}")
                self.refresh_models()
                if auto_load:
                    self.selected_model_name = model_id
                    self.load_selected_model()

            self.root.after(0, finalize)

        threading.Thread(target=task, daemon=True).start()

    def send_message(self) -> None:
        message = self.chat_prompt.get("1.0", "end").strip()
        if not message:
            messagebox.showwarning("Chat", "Enter a message first.")
            return

        mcp_raw = self.tool_access_input.get("1.0", "end").strip()
        mcp_payload = None
        if mcp_raw:
            try:
                mcp_payload = json.loads(mcp_raw)
            except json.JSONDecodeError as exc:
                messagebox.showerror("Tool Access", f"Invalid MCP JSON: {exc}")
                return

        payload = {
            "message": message,
            "temperature": self.temperature_var.get(),
            "top_p": self.top_p_var.get(),
            "top_k": self.top_k_var.get(),
            "penalty": self.penalty_var.get(),
            "max_tokens": self.max_tokens_var.get(),
            "system_prompt": self.system_prompt.get("1.0", "end").strip(),
        }

        if mcp_payload is not None:
            payload["mcp"] = mcp_payload

        self.chat_response.insert("end", f"You: {message}\n\n")
        self.chat_prompt.delete("1.0", "end")

        def task() -> None:
            result = self.api_call("/api/inference/chat", "POST", payload)
            self.root.after(0, lambda: self.update_chat_response(result))

        threading.Thread(target=task, daemon=True).start()

    def update_chat_response(self, result: dict) -> None:
        if result.get("error"):
            self.chat_response.insert("end", f"Error: {result['error']}\n\n")
            return

        response = result.get("response", "No response received")
        request_id = result.get("request_id")
        self.chat_response.insert("end", f"Assistant: {response}\n")
        if request_id:
            self.chat_response.insert("end", f"Request: {request_id}\n")
        self.chat_response.insert("end", "\n")
        self.chat_response.see("end")

    def refresh_metrics(self) -> None:
        def task() -> None:
            result = self.api_call("/api/metrics")
            self.root.after(0, lambda: self.update_metrics_ui(result))

        threading.Thread(target=task, daemon=True).start()

    def update_metrics_ui(self, result: dict) -> None:
        if result.get("error"):
            return

        metrics = result.get("metrics", {})
        self.metric_vars["throughput"].set(str(metrics.get("throughput", 0)))
        self.metric_vars["cpu"].set(f"{metrics.get('cpu', 0)}%")
        self.metric_vars["memory"].set(f"{metrics.get('memory', 0)}%")
        self.metric_vars["gpu"].set(f"{metrics.get('gpu', 0)}%")
        self.metric_vars["latency_p50"].set(f"{metrics.get('latency_p50', 0)} ms")
        self.metric_vars["latency_p95"].set(f"{metrics.get('latency_p95', 0)} ms")
        self.metrics_log.delete("1.0", "end")
        self.metrics_log.insert("end", json.dumps(metrics, indent=2))

    def refresh_sessions(self) -> None:
        def task() -> None:
            result = self.api_call("/api/sessions")
            self.root.after(0, lambda: self.update_sessions_ui(result))

        threading.Thread(target=task, daemon=True).start()

    def update_sessions_ui(self, result: dict) -> None:
        if result.get("error"):
            return

        for item in self.sessions_tree.get_children():
            self.sessions_tree.delete(item)

        for session in result.get("sessions", []):
            self.sessions_tree.insert("", "end", values=(
                session.get("id", ""),
                session.get("model", ""),
                session.get("status", ""),
                session.get("throughput", ""),
                session.get("latency", ""),
                session.get("tokens", ""),
            ))

    def cancel_selected_session(self) -> None:
        selection = self.sessions_tree.selection()
        if not selection:
            return
        session_id = self.sessions_tree.item(selection[0]).get("values", [""])[0]
        if not session_id:
            return

        def task() -> None:
            result = self.api_call(f"/api/sessions/{session_id}/cancel", "POST")
            self.root.after(0, lambda: self.handle_simple_result(result, f"Cancelled session: {session_id}"))

        threading.Thread(target=task, daemon=True).start()

    def refresh_workers(self) -> None:
        def task() -> None:
            result = self.api_call("/api/workers")
            self.root.after(0, lambda: self.update_workers_ui(result))

        threading.Thread(target=task, daemon=True).start()

    def update_workers_ui(self, result: dict) -> None:
        if result.get("error"):
            return

        for item in self.workers_tree.get_children():
            self.workers_tree.delete(item)

        for worker in result.get("workers", []):
            self.workers_tree.insert("", "end", values=(
                worker.get("id", ""),
                worker.get("host", ""),
                worker.get("port", ""),
                worker.get("status", ""),
                worker.get("model", ""),
                worker.get("threads", ""),
                worker.get("load", ""),
            ))

    def add_worker(self) -> None:
        host = self.worker_host_var.get().strip()
        try:
            port = int(self.worker_port_var.get().strip())
        except ValueError:
            messagebox.showwarning("Workers", "Port must be a number.")
            return

        def task() -> None:
            result = self.api_call("/api/workers/add", "POST", {"host": host, "port": port})
            self.root.after(0, lambda: self.handle_simple_result(result, f"Added worker: {host}:{port}"))

        threading.Thread(target=task, daemon=True).start()

    def connect_workers(self) -> None:
        def task() -> None:
            result = self.api_call("/api/workers/connect", "POST")
            self.root.after(0, lambda: self.handle_simple_result(result, "Worker connectivity refreshed"))

        threading.Thread(target=task, daemon=True).start()

    def refresh_jwt(self) -> None:
        def task() -> None:
            result = self.api_call("/api/security/jwt/refresh", "POST")
            self.root.after(0, lambda: self.handle_security_result(result, "JWT refreshed"))

        threading.Thread(target=task, daemon=True).start()

    def enable_pqc(self) -> None:
        def task() -> None:
            result = self.api_call("/api/security/pqc/enable", "POST")
            self.root.after(0, lambda: self.handle_security_result(result, "PQC enabled"))

        threading.Thread(target=task, daemon=True).start()

    def handle_simple_result(self, result: dict, success_message: str) -> None:
        if result.get("error"):
            messagebox.showerror("Ghostlink", result["error"])
            return
        self.header_status.configure(text=success_message)
        self.refresh_models()
        self.refresh_sessions()
        self.refresh_workers()
        self.refresh_metrics()

    def handle_security_result(self, result: dict, success_message: str) -> None:
        if result.get("error"):
            self.security_log.insert("end", f"Error: {result['error']}\n")
            return
        self.security_log.insert("end", f"{success_message}: {json.dumps(result)}\n")
        self.security_log.see("end")

    def launch(self) -> int:
        self.root.mainloop()
        return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Launch Ghostlink Studio")
    parser.add_argument("--host", default="127.0.0.1", help="Backend host")
    parser.add_argument("--port", default="8003", help="Backend port")
    parser.add_argument("--backend-url", default=None, help="Full backend base URL")
    parser.add_argument("--check", action="store_true", help="Run a non-interactive readiness check")
    return parser


def run_check(api_base: str) -> int:
    print(f"Tkinter frontend ready for {api_base}")
    try:
        response = requests.get(f"{api_base}/health", timeout=1.0)
        print(f"Backend health: HTTP {response.status_code}")
    except Exception as exc:
        print(f"Backend health: unavailable ({exc})")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args, _forwarded = parser.parse_known_args(argv)

    api_base = args.backend_url or f"http://{args.host}:{args.port}"
    api_base = api_base.rstrip("/")

    if args.check:
        return run_check(api_base)

    root = tk.Tk()
    app = GhostlinkGUI(root, api_base=api_base)
    return app.launch()


if __name__ == "__main__":
    raise SystemExit(main())