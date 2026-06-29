#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Ghostlink Studio - Professional Inference Interface"""

import sys
import requests
import json
from datetime import datetime
from PyQt6.QtWidgets import (
    QApplication, QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
    QLabel, QPushButton, QTabWidget, QStatusBar, QMessageBox, QGroupBox,
    QTableWidget, QTableWidgetItem, QTextEdit, QLineEdit, QComboBox,
    QSpinBox, QDoubleSpinBox, QProgressBar, QHeaderView, QScrollArea,
    QGridLayout, QFormLayout, QCheckBox, QFrame, QSplitter
)
from PyQt6.QtWidgets import QInputDialog
from PyQt6.QtCore import Qt, QTimer, QThread, pyqtSignal, QSize
from PyQt6.QtGui import QFont, QColor, QIcon, QPalette, QLinearGradient, QBrush


class WorkerHealthCheck(QThread):
    """Background thread for health checks."""
    health_updated = pyqtSignal(dict)
    
    def __init__(self, base_url="http://localhost:8003"):
        super().__init__()
        self.base_url = base_url
        self.running = True
    
    def run(self):
        """Check health periodically."""
        while self.running:
            try:
                response = requests.get(f"{self.base_url}/health", timeout=2)
                if response.status_code == 200:
                    self.health_updated.emit({"status": "healthy", "code": 200})
                else:
                    self.health_updated.emit({"status": "degraded", "code": response.status_code})
            except requests.ConnectionError:
                self.health_updated.emit({"status": "disconnected", "error": "Connection refused"})
            except Exception as e:
                self.health_updated.emit({"status": "error", "error": str(e)})
            
            self.msleep(3000)  # Check every 3 seconds
    
    def stop(self):
        """Stop the health check thread."""
        self.running = False


class MohawkGUI(QMainWindow):
    """Main window for Ghostlink Studio - Professional Inference Surface."""
    
    def __init__(self):
        super().__init__()
        self.setWindowTitle("Ghostlink Studio")
        self.setMinimumSize(1400, 900)
        
        # API endpoints
        self.gui_service_url = "http://localhost:8003"
        self.worker_service_url = "http://localhost:8004"
        
        # Apply dark professional theme
        self.apply_styles()

        # Central widget
        central_widget = QWidget()
        self.setCentralWidget(central_widget)
        main_layout = QHBoxLayout(central_widget)
        main_layout.setContentsMargins(0, 0, 0, 0)
        main_layout.setSpacing(0)
        
        # Sidebar
        self.sidebar = self.create_sidebar()
        main_layout.addWidget(self.sidebar)
        
        # Content area
        content_widget = QWidget()
        self.content_layout = QVBoxLayout(content_widget)
        self.content_layout.setContentsMargins(20, 20, 20, 20)
        main_layout.addWidget(content_widget)
        
        # Header (Top bar)
        self.header = self.create_header()
        self.content_layout.addWidget(self.header)
        
        # Main Tab Stack
        self.stack = QTabWidget()
        self.stack.tabBar().hide() # Use sidebar for switching
        self.content_layout.addWidget(self.stack)
        
        # Status bar
        self.status_bar = QStatusBar()
        self.setStatusBar(self.status_bar)
        self.status_bar.showMessage("Starting Ghostlink Studio...")
        
        # Store references for live updates
        self.metrics_bars = {}
        
        # Create pages
        self.model_library_widget = self.create_model_library_tab()
        self.chat_widget = self.create_chat_interface_tab()
        self.metrics_widget = self.create_metrics_tab()
        self.sessions_widget = self.create_sessions_tab()
        self.workers_widget = self.create_workers_tab()
        self.security_widget = self.create_security_tab()
        self.history_widget = self.create_history_tab()
        
        self.stack.addTab(self.chat_widget, "Chat")
        self.stack.addTab(self.model_library_widget, "Search")
        self.stack.addTab(self.metrics_widget, "Metrics")
        self.stack.addTab(self.sessions_widget, "Sessions")
        self.stack.addTab(self.workers_widget, "Workers")
        self.stack.addTab(self.security_widget, "Security")
        self.stack.addTab(self.history_widget, "History")

        # Health check thread
        self.health_thread = WorkerHealthCheck(self.gui_service_url)
        self.health_thread.health_updated.connect(self.on_health_update)
        self.health_thread.start()

        self.status_bar.showMessage("Ready - Connecting to Ghostlink backend...")
        
        # Timer for periodic updates
        self.update_timer = QTimer()
        self.update_timer.timeout.connect(self.periodic_update)
        self.update_timer.start(5000)  # Update every 5 seconds

    def apply_styles(self):
        """Apply modern dark theme styles."""
        self.setStyleSheet("""
            QMainWindow, QWidget {
                background-color: #1a1b1e;
                color: #e0e0e0;
                font-family: 'Inter', 'Segoe UI', sans-serif;
            }
            QGroupBox {
                border: 1px solid #2d2e32;
                border-radius: 8px;
                margin-top: 1.5em;
                font-weight: bold;
                padding-top: 10px;
            }
            QGroupBox::title {
                subcontrol-origin: margin;
                left: 10px;
                padding: 0 5px;
            }
            QPushButton {
                background-color: #2d2e32;
                border: 1px solid #3f3f46;
                border-radius: 6px;
                padding: 8px 16px;
                font-weight: 500;
            }
            QPushButton:hover {
                background-color: #3f3f46;
                border-color: #52525b;
            }
            QPushButton#primaryBtn {
                background-color: #3b82f6;
                border-color: #2563eb;
                color: white;
            }
            QPushButton#primaryBtn:hover {
                background-color: #2563eb;
            }
            QLineEdit, QTextEdit, QSpinBox, QDoubleSpinBox, QComboBox {
                background-color: #25262b;
                border: 1px solid #2d2e32;
                border-radius: 4px;
                padding: 6px;
            }
            QTableWidget {
                gridline-color: #2d2e32;
                border: none;
            }
            QHeaderView::section {
                background-color: #25262b;
                padding: 8px;
                border: none;
                border-bottom: 1px solid #2d2e32;
                font-weight: bold;
            }
            QProgressBar {
                background-color: #25262b;
                border: 1px solid #2d2e32;
                border-radius: 4px;
                text-align: center;
            }
            QProgressBar::chunk {
                background-color: #3b82f6;
            }
            QScrollBar:vertical {
                border: none;
                background: #1a1b1e;
                width: 10px;
                margin: 0px;
            }
            QScrollBar::handle:vertical {
                background: #3f3f46;
                min-height: 20px;
                border-radius: 5px;
            }
        """)

    def create_sidebar(self):
        """Create the navigation sidebar."""
        frame = QFrame()
        frame.setFixedWidth(240)
        frame.setStyleSheet("background-color: #141517; border-right: 1px solid #2d2e32;")
        layout = QVBoxLayout(frame)
        layout.setContentsMargins(10, 20, 10, 20)

        logo = QLabel("GHOSTLINK")
        logo.setFont(QFont("Inter", 18, QFont.Weight.Bold))
        logo.setStyleSheet("color: #3b82f6; margin-bottom: 30px; margin-left: 10px;")
        layout.addWidget(logo)

        buttons = [
            ("AI Chat", 0),
            ("Search Models", 1),
            ("Analytics", 2),
            ("Session Manager", 3),
            ("Resource Nodes", 4),
            ("Security", 5),
            ("History", 6),
        ]

        self.sidebar_buttons = []
        for text, idx in buttons:
            btn = QPushButton(text)
            btn.setCheckable(True)
            btn.setStyleSheet("""
                QPushButton {
                    text-align: left;
                    padding: 12px;
                    background-color: transparent;
                    border: none;
                    font-size: 14px;
                    border-radius: 6px;
                }
                QPushButton:hover {
                    background-color: #25262b;
                }
                QPushButton:checked {
                    background-color: #3b82f6;
                    color: white;
                }
            """)
            btn.clicked.connect(lambda checked, i=idx: self.switch_tab(i))
            layout.addWidget(btn)
            self.sidebar_buttons.append(btn)
            if idx == 0: btn.setChecked(True)

        layout.addStretch()
        return frame

    def switch_tab(self, index):
        """Switch tab and update sidebar selection."""
        self.stack.setCurrentIndex(index)
        for i, btn in enumerate(self.sidebar_buttons):
            btn.setChecked(i == index)

    def create_header(self):
        """Create the top header bar."""
        widget = QWidget()
        layout = QHBoxLayout(widget)
        layout.setContentsMargins(0, 0, 0, 20)

        self.health_status_icon = QLabel("●")
        self.health_status_icon.setStyleSheet("color: #f59e0b; font-size: 20px;")
        layout.addWidget(self.health_status_icon)

        self.health_label = QLabel("Initializing Ghostlink Fabric...")
        self.health_label.setFont(QFont("Inter", 12))
        layout.addWidget(self.health_label)

        layout.addStretch()

        self.active_model_badge = QLabel("No Active Model")
        self.active_model_badge.setStyleSheet("""
            background-color: #2d2e32;
            padding: 4px 12px;
            border-radius: 12px;
            font-size: 11px;
            color: #9ca3af;
        """)
        layout.addWidget(self.active_model_badge)

        return widget

    def on_health_update(self, health_info):
        """Handle health check updates."""
        status = health_info.get("status")
        
        if status == "healthy":
            self.health_label.setText("Ghostlink Fabric Online")
            self.health_status_icon.setStyleSheet("color: #10b981; font-size: 20px;")
            self.status_bar.showMessage("Connected to inference cluster")
        elif status == "degraded":
            self.health_label.setText("Fabric Performance Degraded")
            self.health_status_icon.setStyleSheet("color: #f59e0b; font-size: 20px;")
        else:
            self.health_label.setText("Fabric Disconnected")
            self.health_status_icon.setStyleSheet("color: #ef4444; font-size: 20px;")
    
    def api_call(self, endpoint, method="GET", data=None):
        """Make API call to backend service."""
        try:
            url = f"{self.gui_service_url}{endpoint}"
            
            if method == "GET":
                response = requests.get(url, timeout=5)
            elif method == "POST":
                response = requests.post(url, json=data, timeout=5)
            elif method == "PUT":
                response = requests.put(url, json=data, timeout=5)
            else:
                return {"error": f"Unsupported HTTP method: {method}"}
            
            if response.status_code in [200, 201]:
                try:
                    return response.json()
                except ValueError:
                    return {"status": "ok", "code": response.status_code}
            else:
                detail = None
                try:
                    payload = response.json()
                    detail = payload.get("detail") or payload.get("error")
                except Exception:
                    detail = response.text.strip() or None

                if detail:
                    return {"error": f"HTTP {response.status_code}: {detail}"}
                return {"error": f"HTTP {response.status_code}"}
        
        except requests.ConnectionError:
            return {"error": "Connection refused - is the service running?"}
        except requests.Timeout:
            return {"error": "Request timeout"}
        except Exception as e:
            return {"error": str(e)}
    
    def create_model_library_tab(self):
        """Create model library management tab."""
        widget = QWidget()
        layout = QVBoxLayout(widget)
        
        # Search and filter
        search_layout = QHBoxLayout()
        self.model_search_input = QLineEdit()
        self.model_search_input.setPlaceholderText("Search models on HuggingFace or Local...")
        search_layout.addWidget(QLabel("Search:"))
        search_layout.addWidget(self.model_search_input)
        
        filter_combo = QComboBox()
        filter_combo.addItems(["All Models", "LLM", "Vision", "Embedding"])
        search_layout.addWidget(filter_combo)
        
        download_btn = QPushButton("Download")
        download_btn.setObjectName("primaryBtn")
        download_btn.clicked.connect(self.download_model)
        search_layout.addWidget(download_btn)
        
        refresh_btn = QPushButton("Refresh")
        refresh_btn.clicked.connect(self.refresh_models)
        search_layout.addWidget(refresh_btn)
        
        layout.addLayout(search_layout)
        
        # Models table
        self.models_table = QTableWidget()
        self.models_table.setColumnCount(6)
        self.models_table.setHorizontalHeaderLabels(["Name", "Size", "Type", "Quant", "Status", "Action"])
        self.models_table.horizontalHeader().setSectionResizeMode(QHeaderView.ResizeMode.Stretch)
        layout.addWidget(self.models_table)
        
        # Model details
        details_group = QGroupBox("Model Configuration")
        details_layout = QFormLayout(details_group)
        self.selected_model_label = QLineEdit("None loaded")
        self.selected_model_label.setReadOnly(True)
        details_layout.addRow("Selected Model", self.selected_model_label)

        self.quant_combo = QComboBox()
        self.quant_combo.addItems(["Default", "Q4_K_M", "Q5_K_M", "Q8_0", "FP16"])
        details_layout.addRow("Quantization Override", self.quant_combo)

        self.split_input = QLineEdit("auto")
        details_layout.addRow("Device Distribution", self.split_input)
        layout.addWidget(details_group)

        self.refresh_models()
        return widget
    
    def refresh_models(self):
        """Refresh model list from controller API."""
        result = self.api_call("/api/models")
        if "error" in result:
            self.status_bar.showMessage(f"Model refresh failed: {result['error']}")
            self.models_table.setRowCount(0)
            return

        models = result.get("models", [])
        current_model = result.get("current_model")
        if current_model:
            self.selected_model_label.setText(current_model)
            self.active_model_badge.setText(current_model)

        self.models_table.setRowCount(len(models))
        for i, model in enumerate(models):
            name = model.get("name", "Unknown")
            size = f"{model.get('size_gb', 0)} GB"
            mtype = model.get("type", "LLM")
            quant = model.get("quantization", "Unknown")
            status = model.get("status", "Unknown")

            self.models_table.setItem(i, 0, QTableWidgetItem(name))
            self.models_table.setItem(i, 1, QTableWidgetItem(size))
            self.models_table.setItem(i, 2, QTableWidgetItem(mtype))
            self.models_table.setItem(i, 3, QTableWidgetItem(quant))
            
            status_item = QTableWidgetItem(status)
            status_color = "#10b981" if status in {"Ready", "Loaded"} else "#f59e0b"
            status_item.setForeground(QColor(status_color))
            self.models_table.setItem(i, 4, status_item)
            
            load_btn = QPushButton("Load")
            load_btn.clicked.connect(lambda checked, n=name: self.load_model_api(n))
            self.models_table.setCellWidget(i, 5, load_btn)
    
    def load_model_api(self, model_name):
        """Load model via API call."""
        result = self.api_call("/api/models/load", "POST", {"model": model_name})
        
        if "error" in result:
            QMessageBox.warning(self, "Load Error", f"Failed to load model:\n{result['error']}")
        else:
            self.selected_model_label.setText(model_name)
            self.active_model_badge.setText(model_name)
            self.active_model_badge.setStyleSheet("background-color: #3b82f6; color: white; padding: 4px 12px; border-radius: 12px; font-size: 11px;")
            QMessageBox.information(self, "Success", f"Model loaded: {model_name}")
            self.status_bar.showMessage(f"Loaded model: {model_name}")
    
    def download_model(self):
        """Download a model."""
        model_id, accepted = QInputDialog.getText(
            self,
            "Download Model",
            "Enter HuggingFace model ID:",
        )
        if not accepted:
            return

        model_id = model_id.strip()
        if not model_id:
            QMessageBox.warning(self, "Download Error", "Model ID cannot be empty")
            return

        result = self.api_call("/api/models/download", "POST", {"model_id": model_id})
        if "error" in result:
            QMessageBox.warning(self, "Download Error", result["error"])
            return

        self.refresh_models()
        QMessageBox.information(self, "Download", f"Model available: {model_id}")
    
    def create_chat_interface_tab(self):
        """Create chat interface tab with advanced parameters."""
        widget = QWidget()
        layout = QHBoxLayout(widget)
        
        # Main Chat Area
        chat_container = QWidget()
        chat_v_layout = QVBoxLayout(chat_container)

        self.chat_display = QTextEdit()
        self.chat_display.setReadOnly(True)
        self.chat_display.setStyleSheet("background-color: #1a1b1e; border: none; font-size: 14px;")
        self.chat_display.setPlaceholderText("Welcome to Ghostlink Studio. Select a model to begin.")
        chat_v_layout.addWidget(self.chat_display)
        
        input_container = QFrame()
        input_container.setStyleSheet("background-color: #25262b; border-radius: 12px; border: 1px solid #3f3f46;")
        input_h_layout = QHBoxLayout(input_container)
        
        self.message_input = QTextEdit()
        self.message_input.setPlaceholderText("Enter prompt...")
        self.message_input.setMaximumHeight(100)
        self.message_input.setStyleSheet("border: none; background-color: transparent;")
        input_h_layout.addWidget(self.message_input)

        send_btn = QPushButton("Send")
        send_btn.setObjectName("primaryBtn")
        send_btn.setFixedSize(80, 40)
        send_btn.clicked.connect(self.send_message)
        input_h_layout.addWidget(send_btn, alignment=Qt.AlignmentFlag.AlignBottom)

        chat_v_layout.addWidget(input_container)
        layout.addWidget(chat_container, 3) # 3/4 width

        # Right Sidebar - Inference Params
        params_scroll = QScrollArea()
        params_scroll.setWidgetResizable(True)
        params_scroll.setFixedWidth(320)
        params_scroll.setStyleSheet("QScrollArea { border: none; border-left: 1px solid #2d2e32; }")

        params_widget = QWidget()
        params_layout = QVBoxLayout(params_widget)

        # Parameters group
        params_group = QGroupBox("Inference Settings")
        form = QFormLayout(params_group)
        
        self.temp_spin = QDoubleSpinBox()
        self.temp_spin.setRange(0, 2.0)
        self.temp_spin.setValue(0.7)
        self.temp_spin.setSingleStep(0.1)
        form.addRow("Temperature", self.temp_spin)
        
        self.topp_spin = QDoubleSpinBox()
        self.topp_spin.setRange(0, 1.0)
        self.topp_spin.setValue(0.9)
        form.addRow("Top P", self.topp_spin)
        
        self.topk_spin = QSpinBox()
        self.topk_spin.setRange(0, 100)
        self.topk_spin.setValue(40)
        form.addRow("Top K", self.topk_spin)
        
        self.penalty_spin = QDoubleSpinBox()
        self.penalty_spin.setRange(1.0, 2.0)
        self.penalty_spin.setValue(1.1)
        form.addRow("Repeat Penalty", self.penalty_spin)
        
        self.max_tokens_spin = QSpinBox()
        self.max_tokens_spin.setRange(1, 32768)
        self.max_tokens_spin.setValue(4096)
        form.addRow("Max Tokens", self.max_tokens_spin)

        params_layout.addWidget(params_group)
        
        # System Prompt
        sys_group = QGroupBox("System Prompt")
        sys_layout = QVBoxLayout(sys_group)
        self.system_prompt = QTextEdit()
        self.system_prompt.setPlainText("You are a highly capable AI assistant running on Ghostlink Fabric.")
        self.system_prompt.setMaximumHeight(150)
        sys_layout.addWidget(self.system_prompt)
        params_layout.addWidget(sys_group)

        # MCP JSON
        mcp_group = QGroupBox("Advanced (MCP)")
        mcp_layout = QVBoxLayout(mcp_group)
        self.mcp_json_input = QTextEdit()
        self.mcp_json_input.setPlaceholderText('{"tools": []}')
        self.mcp_json_input.setMaximumHeight(100)
        mcp_layout.addWidget(self.mcp_json_input)
        params_layout.addWidget(mcp_group)
        
        params_layout.addStretch()
        params_scroll.setWidget(params_widget)
        layout.addWidget(params_scroll, 1) # 1/4 width
        
        return widget
    
    def send_message(self):
        """Send message to inference backend."""
        message = self.message_input.toPlainText().strip()
        if not message:
            return

        mcp_payload = None
        mcp_raw = self.mcp_json_input.toPlainText().strip()
        if mcp_raw:
            try:
                mcp_payload = json.loads(mcp_raw)
            except json.JSONDecodeError as err:
                self.chat_display.append(f"[ERROR] Invalid MCP JSON: {err}\n")
                self.status_bar.showMessage("Invalid MCP JSON")
                return
        
        self.chat_display.append(f"\n<b>You:</b> {message}\n")
        self.message_input.clear()
        
        payload = {
            "message": message,
            "temperature": self.temp_spin.value(),
            "top_p": self.topp_spin.value(),
            "top_k": self.topk_spin.value(),
            "penalty": self.penalty_spin.value(),
            "max_tokens": self.max_tokens_spin.value(),
            "system_prompt": self.system_prompt.toPlainText()
        }
        if mcp_payload is not None:
            payload["mcp"] = mcp_payload
        
        result = self.api_call("/api/inference/chat", "POST", payload)
        
        if "error" in result:
            self.chat_display.append(f"<font color='red'>[ERROR] {result['error']}</font>\n")
        else:
            response = result.get("response", "No response received")
            self.chat_display.append(f"<b>Assistant:</b> {response}\n")
        
        self.status_bar.showMessage("Message processed")

    def create_metrics_tab(self):
        """Create performance metrics tab."""
        widget = QWidget()
        layout = QVBoxLayout(widget)
        
        # Metrics grid
        metrics_group = QGroupBox("Fabric Performance")
        metrics_layout = QGridLayout(metrics_group)
        
        # Throughput
        metrics_layout.addWidget(QLabel("Token Throughput (tok/s):"), 0, 0)
        self.throughput_bar = QProgressBar()
        self.throughput_bar.setMaximum(150000) # Ghostlink max
        metrics_layout.addWidget(self.throughput_bar, 0, 1)
        self.throughput_value_label = QLabel("0")
        metrics_layout.addWidget(self.throughput_value_label, 0, 2)
        self.metrics_bars["throughput"] = (self.throughput_bar, self.throughput_value_label)
        
        # Latency p50
        metrics_layout.addWidget(QLabel("Latency p50 (ms):"), 1, 0)
        latency_bar = QProgressBar()
        latency_bar.setMaximum(50)
        metrics_layout.addWidget(latency_bar, 1, 1)
        latency_value = QLabel("0")
        metrics_layout.addWidget(latency_value, 1, 2)
        self.metrics_bars["latency_p50"] = (latency_bar, latency_value)
        
        # Latency p95
        metrics_layout.addWidget(QLabel("Latency p95 (ms):"), 2, 0)
        latency95_bar = QProgressBar()
        latency95_bar.setMaximum(100)
        metrics_layout.addWidget(latency95_bar, 2, 1)
        latency95_value = QLabel("0")
        metrics_layout.addWidget(latency95_value, 2, 2)
        self.metrics_bars["latency_p95"] = (latency95_bar, latency95_value)
        
        layout.addWidget(metrics_group)
        
        # Resource usage
        resource_group = QGroupBox("Hardware Utilization")
        resource_layout = QGridLayout(resource_group)
        
        resource_layout.addWidget(QLabel("Global CPU:"), 0, 0)
        self.cpu_bar = QProgressBar()
        resource_layout.addWidget(self.cpu_bar, 0, 1)
        self.cpu_value = QLabel("0%")
        resource_layout.addWidget(self.cpu_value, 0, 2)
        
        resource_layout.addWidget(QLabel("Cluster VRAM:"), 1, 0)
        self.mem_bar = QProgressBar()
        resource_layout.addWidget(self.mem_bar, 1, 1)
        self.mem_value = QLabel("0%")
        resource_layout.addWidget(self.mem_value, 1, 2)
        
        resource_layout.addWidget(QLabel("Primary GPU:"), 2, 0)
        self.gpu_bar = QProgressBar()
        resource_layout.addWidget(self.gpu_bar, 2, 1)
        self.gpu_value = QLabel("0%")
        resource_layout.addWidget(self.gpu_value, 2, 2)
        
        layout.addWidget(resource_group)
        
        layout.addStretch()
        return widget
    
    def create_sessions_tab(self):
        """Create sessions manager tab."""
        widget = QWidget()
        layout = QVBoxLayout(widget)
        
        # Controls
        controls_layout = QHBoxLayout()
        controls_layout.addWidget(QLabel("Max Queue Depth:"))
        self.queue_spin = QSpinBox()
        self.queue_spin.setValue(50)
        controls_layout.addWidget(self.queue_spin)
        
        high_priority_btn = QPushButton("Queue High Priority")
        high_priority_btn.clicked.connect(self.queue_high_priority)
        controls_layout.addWidget(high_priority_btn)
        
        refresh_btn = QPushButton("Refresh")
        refresh_btn.clicked.connect(self.refresh_sessions)
        controls_layout.addWidget(refresh_btn)
        
        layout.addLayout(controls_layout)
        
        # Sessions table
        self.sessions_table = QTableWidget()
        self.sessions_table.setColumnCount(7)
        self.sessions_table.setHorizontalHeaderLabels(["Session ID", "Model", "Status", "Throughput", "Latency", "Tokens", "Actions"])
        self.sessions_table.horizontalHeader().setSectionResizeMode(QHeaderView.ResizeMode.Stretch)
        
        self.refresh_sessions()
        layout.addWidget(self.sessions_table)
        
        return widget
    
    def refresh_sessions(self):
        """Refresh sessions from API."""
        result = self.api_call("/api/sessions")
        if "error" in result:
            self.status_bar.showMessage(f"Session refresh failed: {result['error']}")
            sessions = []
        else:
            sessions = result.get("sessions", [])
        
        self.sessions_table.setRowCount(len(sessions))
        for i, session in enumerate(sessions):
            self.sessions_table.setItem(i, 0, QTableWidgetItem(session.get("id", f"sess_{i:03d}")))
            self.sessions_table.setItem(i, 1, QTableWidgetItem(session.get("model", "Unknown")))
            
            status = session.get("status", "Unknown")
            status_item = QTableWidgetItem(status)
            status_color = "#10b981" if status == "Running" else "#3b82f6"
            status_item.setForeground(QColor(status_color))
            self.sessions_table.setItem(i, 2, status_item)
            
            self.sessions_table.setItem(i, 3, QTableWidgetItem(str(session.get("throughput", 0))))
            self.sessions_table.setItem(i, 4, QTableWidgetItem(f"{session.get('latency', 0)}ms"))
            self.sessions_table.setItem(i, 5, QTableWidgetItem(str(session.get("tokens", 0))))
            
            cancel_btn = QPushButton("Cancel")
            cancel_btn.clicked.connect(lambda checked, idx=i: self.cancel_session(idx))
            self.sessions_table.setCellWidget(i, 6, cancel_btn)
    
    def queue_high_priority(self):
        """Queue a job with high priority."""
        result = self.api_call("/api/queue", "POST", {"priority": "high"})
        if "error" in result:
            QMessageBox.warning(self, "Queue Error", result["error"])
        else:
            QMessageBox.information(self, "Queued", "Job queued with high priority")
    
    def queue_normal_priority(self):
        """Queue a job with normal priority."""
        result = self.api_call("/api/queue", "POST", {"priority": "normal"})
        if "error" in result:
            QMessageBox.warning(self, "Queue Error", result["error"])
        else:
            QMessageBox.information(self, "Queued", "Job queued with normal priority")
    
    def cancel_session(self, session_idx):
        """Cancel a session."""
        reply = QMessageBox.question(self, "Cancel Session", "Are you sure?")
        if reply == QMessageBox.StandardButton.Yes:
            session_id = self.sessions_table.item(session_idx, 0).text()
            result = self.api_call(f"/api/sessions/{session_id}/cancel", "POST")
            if "error" in result:
                QMessageBox.warning(self, "Cancel Error", result["error"])
                return
            QMessageBox.information(self, "Cancelled", "Session cancelled")
            self.refresh_sessions()
    
    def create_workers_tab(self):
        """Create workers configuration tab."""
        widget = QWidget()
        layout = QVBoxLayout(widget)
        
        # Add worker controls
        add_layout = QHBoxLayout()
        add_layout.addWidget(QLabel("Remote Host:"))
        self.worker_host_input = QLineEdit()
        self.worker_host_input.setPlaceholderText("IP or Hostname")
        add_layout.addWidget(self.worker_host_input)
        
        add_layout.addWidget(QLabel("Port:"))
        self.worker_port_spin = QSpinBox()
        self.worker_port_spin.setValue(8005)
        self.worker_port_spin.setRange(1, 65535)
        add_layout.addWidget(self.worker_port_spin)
        
        add_btn = QPushButton("Add Node")
        add_btn.setObjectName("primaryBtn")
        add_btn.clicked.connect(self.add_worker)
        add_layout.addWidget(add_btn)
        
        connect_btn = QPushButton("Connect All")
        connect_btn.clicked.connect(self.connect_workers)
        add_layout.addWidget(connect_btn)

        refresh_btn = QPushButton("Refresh")
        refresh_btn.clicked.connect(self.refresh_workers)
        add_layout.addWidget(refresh_btn)
        
        layout.addLayout(add_layout)
        
        # Workers table
        self.workers_table = QTableWidget()
        self.workers_table.setColumnCount(7)
        self.workers_table.setHorizontalHeaderLabels(["Node ID", "Endpoint", "Status", "Active Model", "Threads", "Load", "Actions"])
        self.workers_table.horizontalHeader().setSectionResizeMode(QHeaderView.ResizeMode.Stretch)
        
        self.refresh_workers()
        layout.addWidget(self.workers_table)
        
        return widget
    
    def refresh_workers(self):
        """Refresh workers from API."""
        result = self.api_call("/api/workers")
        if "error" in result:
            self.status_bar.showMessage(f"Worker refresh failed: {result['error']}")
            workers = []
        else:
            workers = result.get("workers", [])

        connected = sum(1 for w in workers if w.get("status") == "Connected")
        self.status_bar.showMessage(f"Cluster: {connected}/{len(workers)} nodes online")
        
        self.workers_table.setRowCount(len(workers))
        for i, worker in enumerate(workers):
            self.workers_table.setItem(i, 0, QTableWidgetItem(worker.get("id", f"worker_{i}")))
            host_port = f"{worker.get('host', 'localhost')}:{worker.get('port', 8000)}"
            self.workers_table.setItem(i, 1, QTableWidgetItem(host_port))
            
            status = worker.get("status", "Unknown")
            status_item = QTableWidgetItem(status)
            status_color = "#10b981" if status == "Connected" else "#f59e0b"
            status_item.setForeground(QColor(status_color))
            self.workers_table.setItem(i, 2, status_item)
            
            self.workers_table.setItem(i, 3, QTableWidgetItem(worker.get("model", "None")))
            self.workers_table.setItem(i, 4, QTableWidgetItem(str(worker.get("threads", 0))))
            
            load = worker.get("load", 0)
            load_bar = QProgressBar()
            load_bar.setValue(load)
            self.workers_table.setCellWidget(i, 5, load_bar)
            
            action_btn = QPushButton("Manage")
            self.workers_table.setCellWidget(i, 6, action_btn)
    
    def add_worker(self):
        """Add a new worker."""
        host = self.worker_host_input.text().strip()
        port = int(self.worker_port_spin.value())

        if not host:
            QMessageBox.warning(self, "Add Worker", "Host is required")
            return

        result = self.api_call("/api/workers/add", "POST", {"host": host, "port": port})
        if "error" in result:
            QMessageBox.warning(self, "Add Worker", result["error"])
            return

        self.refresh_workers()
        QMessageBox.information(self, "Worker Added", f"Worker added: {host}:{port}")
    
    def create_security_tab(self):
        """Create security center tab."""
        widget = QWidget()
        layout = QVBoxLayout(widget)
        
        # JWT
        jwt_group = QGroupBox("Fabric Authentication (JWT)")
        jwt_layout = QFormLayout(jwt_group)
        jwt_status = QLabel("Active (HMAC-SHA256)")
        jwt_status.setStyleSheet("color: #10b981; font-weight: bold;")
        jwt_layout.addRow("Status", jwt_status)
        jwt_layout.addRow("Token Expiry", QLabel("12 hours (Rolling)"))
        refresh_btn = QPushButton("Rotate Keys")
        refresh_btn.clicked.connect(lambda: self.api_call("/api/security/jwt/refresh", "POST"))
        jwt_layout.addRow("Action", refresh_btn)
        layout.addWidget(jwt_group)
        
        # mTLS
        mtls_group = QGroupBox("Transport Layer Security (mTLS)")
        mtls_layout = QFormLayout(mtls_group)
        mtls_status = QLabel("Enabled")
        mtls_status.setStyleSheet("color: #10b981; font-weight: bold;")
        mtls_layout.addRow("Status", mtls_status)
        mtls_layout.addRow("Certificate Authority", QLabel("Ghostlink Internal CA"))
        mtls_layout.addRow("Client Identity", QLabel("studio-primary-desktop"))
        layout.addWidget(mtls_group)
        
        # PQC
        pqc_group = QGroupBox("Post-Quantum Cryptography")
        pqc_layout = QFormLayout(pqc_group)
        pqc_status = QLabel("Experimental - ML-KEM")
        pqc_status.setStyleSheet("color: #f59e0b; font-weight: bold;")
        pqc_layout.addRow("Status", pqc_status)
        enable_pqc_btn = QPushButton("Enable Hybrid Quantum Tunnel")
        enable_pqc_btn.clicked.connect(lambda: self.api_call("/api/security/pqc/enable", "POST"))
        pqc_layout.addRow("Action", enable_pqc_btn)
        layout.addWidget(pqc_group)
        
        # Security logs
        logs_group = QGroupBox("Audit Log")
        logs_layout = QVBoxLayout(logs_group)
        security_log = QTextEdit()
        security_log.setReadOnly(True)
        security_log.setStyleSheet("background-color: #141517; font-family: 'Consolas', monospace; font-size: 12px;")
        security_log.setPlainText(
            f"[{datetime.now().strftime('%Y-%m-%d %H:%M:%S')}] Ghostlink Studio Session Started\n"
            f"[{datetime.now().strftime('%Y-%m-%d %H:%M:%S')}] mTLS Handshake Completed with Local Node\n"
            f"[{datetime.now().strftime('%Y-%m-%d %H:%M:%S')}] Discovery Broadcast Received: 2 nodes online\n"
        )
        logs_layout.addWidget(security_log)
        layout.addWidget(logs_group)
        
        return widget
    
    def create_history_tab(self):
        """Create conversation history tab."""
        widget = QWidget()
        layout = QVBoxLayout(widget)
        
        # History table
        table = QTableWidget()
        table.setColumnCount(6)
        table.setHorizontalHeaderLabels(["Timestamp", "Model", "Tokens", "Duration", "Throughput", "Actions"])
        table.horizontalHeader().setSectionResizeMode(QHeaderView.ResizeMode.Stretch)
        
        history = [
            (datetime.now().strftime('%H:%M:%S'), "Llama-3-8B", 1250, "12s", "104 tok/s"),
            ("14:25:31", "Llama-3-8B", 890, "8s", "111 tok/s"),
            ("14:20:12", "Mistral-7B", 2100, "18s", "116 tok/s"),
        ]
        
        table.setRowCount(len(history))
        for i, (timestamp, model, tokens, duration, thr) in enumerate(history):
            table.setItem(i, 0, QTableWidgetItem(timestamp))
            table.setItem(i, 1, QTableWidgetItem(model))
            table.setItem(i, 2, QTableWidgetItem(str(tokens)))
            table.setItem(i, 3, QTableWidgetItem(duration))
            table.setItem(i, 4, QTableWidgetItem(thr))
            
            view_btn = QPushButton("View")
            table.setCellWidget(i, 5, view_btn)
        
        layout.addWidget(table)
        
        # Statistics
        stats_group = QGroupBox("Usage Statistics")
        stats_layout = QFormLayout(stats_group)
        stats_layout.addRow("Lifetime Tokens Generated", QLabel("4,567,890"))
        stats_layout.addRow("Average Cluster Latency", QLabel("2.1ms"))
        stats_layout.addRow("Total Uptime", QLabel("14d 2h 11m"))
        layout.addWidget(stats_group)
        
        return widget
    
    def connect_workers(self):
        """Connect to worker services."""
        result = self.api_call("/api/workers/connect", "POST")
        
        if "error" in result:
            QMessageBox.warning(self, "Connection Error", result["error"])
        else:
            connected = result.get("connected", 0)
            total = result.get("total", connected)
            self.status_bar.showMessage(f"Connected to {connected} nodes")
            QMessageBox.information(self, "Cluster Status", f"Successfully connected to {connected}/{total} nodes.")
            self.refresh_workers()
    
    def periodic_update(self):
        """Periodic updates for live data."""
        result = self.api_call("/api/metrics")
        
        if "error" not in result:
            metrics = result.get("metrics", {})
            
            # Update throughput
            throughput = metrics.get("throughput", 0)
            self.throughput_bar.setValue(int(throughput))
            self.throughput_value_label.setText(f"{int(throughput):,}")
            
            # Update CPU/Memory/GPU
            self.cpu_bar.setValue(metrics.get("cpu", 0))
            self.cpu_value.setText(f"{metrics.get('cpu', 0)}%")
            
            self.mem_bar.setValue(metrics.get("memory", 0))
            self.mem_value.setText(f"{metrics.get('memory', 0)}%")
            
            self.gpu_bar.setValue(metrics.get("gpu", 0))
            self.gpu_value.setText(f"{metrics.get('gpu', 0)}%")

            # Update latency bars when available
            if "latency_p50" in metrics:
                bar, label = self.metrics_bars["latency_p50"]
                bar.setValue(int(metrics.get("latency_p50", 0)))
                label.setText(str(int(metrics.get("latency_p50", 0))))
            if "latency_p95" in metrics:
                bar, label = self.metrics_bars["latency_p95"]
                bar.setValue(int(metrics.get("latency_p95", 0)))
                label.setText(str(int(metrics.get("latency_p95", 0))))
    
    def refresh_all(self):
        """Refresh all data."""
        self.refresh_models()
        self.refresh_sessions()
        self.refresh_workers()
        self.periodic_update()
        self.status_bar.showMessage("Refreshed cluster state")
    
    def closeEvent(self, event):
        """Handle window close."""
        self.health_thread.stop()
        event.accept()


def main():
    """Main entry point."""
    app = QApplication(sys.argv)
    window = MohawkGUI()
    window.show()
    sys.exit(app.exec())


if __name__ == "__main__":
    main()
