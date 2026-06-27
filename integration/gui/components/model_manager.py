"""
Model Manager Component for Mohawk GUI

Provides model management functionality:
- Model loading from local paths or HuggingFace
- Model switching
- Model information display
- Download progress tracking
"""

import gradio as gr
from typing import List, Dict, Any, Optional
import os


class ModelManager:
    """
    Professional model management component.
    
    Features:
    - Browse and load models
    - HuggingFace integration
    - Model information cards
    - Loading progress indicators
    - Model validation
    """
    
    def __init__(self, server=None):
        """
        Initialize the model manager.
        
        Args:
            server: APIServer instance with engine
        """
        self.server = server
        self.available_models = []
        self.current_model = None
    
    def render(self):
        """Render the model manager component."""
        
        with gr.Column(scale=1) as container:
            # Header
            with gr.Row():
                gr.Markdown("### 📁 Model Management")
            
            # Status overview
            with gr.Row():
                self.status_card = gr.Markdown(
                    self._get_status_markdown(),
                    elem_classes=["metric-card"],
                )
            
            # Load model section
            with gr.Group():
                gr.Markdown("#### Load New Model")
                
                with gr.Row():
                    self.model_source = gr.Radio(
                        choices=[
                            ("🤗 HuggingFace", "huggingface"),
                            ("💾 Local Path", "local"),
                            ("📦 Pre-configured", "preconfigured"),
                        ],
                        value="huggingface",
                        label="Model Source",
                    )
                
                with gr.Row():
                    self.hf_model_id = gr.Textbox(
                        label="HuggingFace Model ID",
                        placeholder="e.g., meta-llama/Llama-2-7b-chat-hf",
                        visible=True,
                    )
                    
                    self.local_path = gr.Textbox(
                        label="Local Model Path",
                        placeholder="/path/to/model",
                        visible=False,
                    )
                    
                    self.preconfigured_model = gr.Dropdown(
                        label="Select Pre-configured Model",
                        choices=[],
                        visible=False,
                    )
                
                with gr.Row():
                    self.load_btn = gr.Button(
                        "⬇️ Load Model",
                        variant="primary",
                        scale=1,
                    )
                    
                    self.cancel_btn = gr.Button(
                        "❌ Cancel",
                        variant="secondary",
                        scale=1,
                    )
                
                # Progress indicator
                self.progress_bar = gr.Slider(
                    label="Loading Progress",
                    minimum=0,
                    maximum=100,
                    value=0,
                    interactive=False,
                    visible=False,
                )
                
                self.progress_text = gr.Markdown(visible=False)
            
            # Current model info
            with gr.Group():
                gr.Markdown("#### Current Model")
                
                with gr.Row():
                    self.model_info = gr.JSON(
                        label="Model Information",
                        value=self._get_model_info(),
                    )
                
                with gr.Row():
                    self.unload_btn = gr.Button(
                        "⏏️ Unload Model",
                        variant="stop",
                    )
            
            # Available models list
            with gr.Group():
                gr.Markdown("#### Available Models")
                
                self.models_table = gr.Dataframe(
                    headers=["Model ID", "Type", "Size", "Status"],
                    datatype=["str", "str", "str", "str"],
                    row_count=5,
                    col_count=4,
                    interactive=False,
                )
                
                with gr.Row():
                    self.refresh_btn = gr.Button("🔄 Refresh List")
                    self.open_folder_btn = gr.Button("📂 Open Models Folder")
            
            # Model settings
            with gr.Group():
                gr.Markdown("#### Model Settings")
                
                with gr.Row():
                    self.auto_unload = gr.Checkbox(
                        label="Auto-unload model when idle (saves memory)",
                        value=False,
                    )
                    
                    self.default_backend = gr.Dropdown(
                        label="Default Backend",
                        choices=["auto", "transformers", "llama-cpp", "onnx"],
                        value="auto",
                    )
        
        # Set up event handlers
        self._setup_events()
        
        return container
    
    def _setup_events(self):
        """Set up event handlers."""
        
        # Toggle input visibility based on source
        self.model_source.change(
            fn=self._toggle_source_inputs,
            inputs=[self.model_source],
            outputs=[self.hf_model_id, self.local_path, self.preconfigured_model],
        )
        
        # Load model
        self.load_btn.click(
            fn=self._load_model,
            inputs=[self.model_source, self.hf_model_id, self.local_path, self.preconfigured_model],
            outputs=[self.progress_bar, self.progress_text, self.status_card, self.model_info],
        )
        
        # Unload model
        self.unload_btn.click(
            fn=self._unload_model,
            inputs=[],
            outputs=[self.status_card, self.model_info],
        )
        
        # Refresh models list
        self.refresh_btn.click(
            fn=self._refresh_models_list,
            inputs=[],
            outputs=[self.models_table],
        )
    
    def _toggle_source_inputs(self, source):
        """Toggle visibility of input fields based on selected source."""
        if source == "huggingface":
            return gr.update(visible=True), gr.update(visible=False), gr.update(visible=False)
        elif source == "local":
            return gr.update(visible=False), gr.update(visible=True), gr.update(visible=False)
        else:  # preconfigured
            return gr.update(visible=False), gr.update(visible=False), gr.update(visible=True)
    
    def _get_status_markdown(self) -> str:
        """Get the current status markdown."""
        if self.server and self.server.engine.is_loaded:
            model_name = self.server.engine.model_path or "Unknown"
            return f"""
            <div style="padding: 16px; border-radius: 8px; background: linear-gradient(135deg, #10B981 0%, #059669 100%);">
                <h3 style="margin: 0 0 8px 0; color: white;">✅ Model Active</h3>
                <p style="margin: 0; color: rgba(255,255,255,0.9);"><strong>Current:</strong> {model_name}</p>
                <p style="margin: 8px 0 0 0; color: rgba(255,255,255,0.8); font-size: 0.9em;">Ready for inference</p>
            </div>
            """
        else:
            return """
            <div style="padding: 16px; border-radius: 8px; background: linear-gradient(135deg, #64748B 0%, #475569 100%);">
                <h3 style="margin: 0 0 8px 0; color: white;">⭕ No Model Loaded</h3>
                <p style="margin: 0; color: rgba(255,255,255,0.9);">Load a model to start generating text</p>
                <p style="margin: 8px 0 0 0; color: rgba(255,255,255,0.8); font-size: 0.9em;">Select a source above and click "Load Model"</p>
            </div>
            """
    
    def _get_model_info(self) -> dict:
        """Get current model information."""
        if self.server and self.server.engine.is_loaded:
            return self.server.engine.get_info()
        return {"status": "No model loaded"}
    
    def _load_model(self, source, hf_id, local_path, preconfigured):
        """Load a model from the specified source."""
        try:
            # Show progress
            yield gr.update(value=10, visible=True), gr.update(value="Initializing...", visible=True), \
                  self._get_status_markdown(), self._get_model_info()
            
            # Determine model path
            if source == "huggingface":
                model_path = hf_id
                if not model_path:
                    yield gr.update(value=0), gr.update(value="❌ Please enter a model ID"), \
                          self._get_status_markdown(), self._get_model_info()
                    return
            elif source == "local":
                model_path = local_path
                if not model_path or not os.path.exists(model_path):
                    yield gr.update(value=0), gr.update(value="❌ Invalid path"), \
                          self._get_status_markdown(), self._get_model_info()
                    return
            else:
                model_path = preconfigured
                if not model_path:
                    yield gr.update(value=0), gr.update(value="❌ Please select a model"), \
                          self._get_status_markdown(), self._get_model_info()
                    return
            
            # Update progress
            yield gr.update(value=40, visible=True), gr.update(value=f"Loading {model_path}...", visible=True), \
                  self._get_status_markdown(), self._get_model_info()
            
            # Load the model
            if self.server:
                self.server.engine.load_model(model_path)
                self.current_model = model_path
            
            # Complete
            yield gr.update(value=100, visible=True), gr.update(value="✅ Model loaded successfully!", visible=True), \
                  self._get_status_markdown(), self._get_model_info()
            
            # Hide progress after delay (in real implementation)
            
        except Exception as e:
            yield gr.update(value=0), gr.update(value=f"❌ Error: {str(e)}"), \
                  self._get_status_markdown(), self._get_model_info()
    
    def _unload_model(self):
        """Unload the current model."""
        if self.server:
            self.server.engine.unload_model()
            self.current_model = None
        
        return self._get_status_markdown(), self._get_model_info()
    
    def _refresh_models_list(self):
        """Refresh the list of available models."""
        # In a real implementation, this would scan the models directory
        # and query HuggingFace for popular models
        sample_models = [
            ["meta-llama/Llama-2-7b-chat-hf", "HuggingFace", "~13 GB", "Available"],
            ["mistralai/Mistral-7B-Instruct-v0.2", "HuggingFace", "~14 GB", "Available"],
            ["TheBloke/Llama-2-7B-GGUF", "HuggingFace", "~4 GB", "Available"],
            ["local-model-1", "Local", "~7 GB", "Loaded"],
        ]
        return gr.update(value=sample_models)
