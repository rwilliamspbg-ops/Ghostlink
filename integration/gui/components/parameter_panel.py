"""
Parameter Panel Component for Mohawk GUI

Provides generation parameter controls:
- Temperature, max tokens, top-p sliders
- Preset configurations
- Advanced parameters
- Real-time parameter display
"""

import gradio as gr
from typing import Dict, Any, Callable


class ParameterPanel:
    """
    Professional parameter control panel.
    
    Features:
    - Sliders with numeric input
    - Preset configurations
    - Parameter validation
    - Real-time updates
    """
    
    # Preset configurations
    PRESETS = {
        "🎯 Precise": {
            "temperature": 0.1,
            "max_tokens": 512,
            "top_p": 0.9,
            "top_k": 1,
            "description": "Deterministic output, best for factual responses",
        },
        "⚖️ Balanced": {
            "temperature": 0.7,
            "max_tokens": 512,
            "top_p": 0.9,
            "top_k": 40,
            "description": "Good balance of creativity and coherence",
        },
        "🎨 Creative": {
            "temperature": 1.2,
            "max_tokens": 1024,
            "top_p": 0.95,
            "top_k": 50,
            "description": "More creative and diverse outputs",
        },
        "🔥 Chaotic": {
            "temperature": 2.0,
            "max_tokens": 2048,
            "top_p": 1.0,
            "top_k": 100,
            "description": "Maximum randomness and creativity",
        },
        "💻 Code": {
            "temperature": 0.2,
            "max_tokens": 1024,
            "top_p": 0.95,
            "top_k": 1,
            "description": "Optimized for code generation",
        },
    }
    
    def __init__(self, on_parameter_change: Callable = None):
        """
        Initialize the parameter panel.
        
        Args:
            on_parameter_change: Callback function when parameters change
        """
        self.on_parameter_change = on_parameter_change
        self.current_params = self.PRESETS["⚖️ Balanced"].copy()
    
    def render(self):
        """Render the parameter panel component."""
        
        with gr.Column(scale=1) as container:
            # Header
            with gr.Row():
                gr.Markdown("### ⚙️ Generation Parameters")
            
            # Preset selector
            with gr.Group():
                gr.Markdown("#### Quick Presets")
                
                self.preset_dropdown = gr.Dropdown(
                    choices=list(self.PRESETS.keys()),
                    value="⚖️ Balanced",
                    label="Select Preset",
                )
                
                self.preset_description = gr.Markdown(
                    self.PRESETS["⚖️ Balanced"]["description"]
                )
            
            # Main parameters
            with gr.Group():
                gr.Markdown("#### Core Parameters")
                
                # Temperature
                with gr.Row():
                    self.temperature_slider = gr.Slider(
                        minimum=0.0,
                        maximum=2.0,
                        value=0.7,
                        step=0.01,
                        label="Temperature",
                        info="Controls randomness (0.0 = deterministic, 2.0 = chaotic)",
                    )
                    self.temperature_num = gr.Number(
                        value=0.7,
                        label="",
                        precision=2,
                        container=True,
                    )
                
                # Max tokens
                with gr.Row():
                    self.max_tokens_slider = gr.Slider(
                        minimum=1,
                        maximum=4096,
                        value=512,
                        step=1,
                        label="Max Tokens",
                        info="Maximum number of tokens to generate",
                    )
                    self.max_tokens_num = gr.Number(
                        value=512,
                        label="",
                        precision=0,
                        container=True,
                    )
                
                # Top-P (Nucleus sampling)
                with gr.Row():
                    self.top_p_slider = gr.Slider(
                        minimum=0.0,
                        maximum=1.0,
                        value=0.9,
                        step=0.01,
                        label="Top-P (Nucleus Sampling)",
                        info="Cumulative probability threshold",
                    )
                    self.top_p_num = gr.Number(
                        value=0.9,
                        label="",
                        precision=2,
                        container=True,
                    )
                
                # Top-K
                with gr.Row():
                    self.top_k_slider = gr.Slider(
                        minimum=1,
                        maximum=100,
                        value=40,
                        step=1,
                        label="Top-K",
                        info="Sample from top K tokens (1 = greedy)",
                    )
                    self.top_k_num = gr.Number(
                        value=40,
                        label="",
                        precision=0,
                        container=True,
                    )
            
            # Advanced parameters
            with gr.Group():
                with gr.Accordion("Advanced Parameters", open=False):
                    gr.Markdown("#### Advanced Settings")
                    
                    # Repetition penalty
                    self.repetition_penalty = gr.Slider(
                        minimum=0.5,
                        maximum=2.0,
                        value=1.1,
                        step=0.05,
                        label="Repetition Penalty",
                        info="Penalize repeated tokens (>1.0 reduces repetition)",
                    )
                    
                    # Stop sequences
                    self.stop_sequences = gr.Textbox(
                        label="Stop Sequences (comma-separated)",
                        placeholder="\n\n, ###, [END]",
                        lines=2,
                        info="Generation stops when any of these sequences are encountered",
                    )
                    
                    # Seed
                    with gr.Row():
                        self.seed_checkbox = gr.Checkbox(
                            label="Use fixed seed for reproducibility",
                            value=False,
                        )
                        self.seed_input = gr.Number(
                            label="Seed value",
                            value=42,
                            precision=0,
                            visible=False,
                        )
            
            # Parameter summary
            with gr.Group():
                gr.Markdown("#### Current Configuration")
                
                self.params_summary = gr.JSON(
                    value=self._get_current_params(),
                    label="Active Parameters",
                )
                
                self.apply_btn = gr.Button(
                    "✅ Apply Parameters",
                    variant="primary",
                )
                
                self.reset_btn = gr.Button(
                    "🔄 Reset to Defaults",
                    variant="secondary",
                )
        
        # Set up event handlers
        self._setup_events()
        
        return container
    
    def _setup_events(self):
        """Set up event handlers for user interactions."""
        
        # Sync sliders with number inputs
        self.temperature_slider.change(
            fn=lambda x: gr.update(value=x),
            inputs=[self.temperature_slider],
            outputs=[self.temperature_num],
        )
        self.temperature_num.change(
            fn=lambda x: gr.update(value=x),
            inputs=[self.temperature_num],
            outputs=[self.temperature_slider],
        )
        
        self.max_tokens_slider.change(
            fn=lambda x: gr.update(value=x),
            inputs=[self.max_tokens_slider],
            outputs=[self.max_tokens_num],
        )
        self.max_tokens_num.change(
            fn=lambda x: gr.update(value=x),
            inputs=[self.max_tokens_num],
            outputs=[self.max_tokens_slider],
        )
        
        self.top_p_slider.change(
            fn=lambda x: gr.update(value=x),
            inputs=[self.top_p_slider],
            outputs=[self.top_p_num],
        )
        self.top_p_num.change(
            fn=lambda x: gr.update(value=x),
            inputs=[self.top_p_num],
            outputs=[self.top_p_slider],
        )
        
        self.top_k_slider.change(
            fn=lambda x: gr.update(value=x),
            inputs=[self.top_k_slider],
            outputs=[self.top_k_num],
        )
        self.top_k_num.change(
            fn=lambda x: gr.update(value=x),
            inputs=[self.top_k_num],
            outputs=[self.top_k_slider],
        )
        
        # Preset selection
        self.preset_dropdown.change(
            fn=self._apply_preset,
            inputs=[self.preset_dropdown],
            outputs=[
                self.temperature_slider, self.temperature_num,
                self.max_tokens_slider, self.max_tokens_num,
                self.top_p_slider, self.top_p_num,
                self.top_k_slider, self.top_k_num,
                self.preset_description,
                self.params_summary,
            ],
        )
        
        # Toggle seed input visibility
        self.seed_checkbox.change(
            fn=lambda x: gr.update(visible=x),
            inputs=[self.seed_checkbox],
            outputs=[self.seed_input],
        )
        
        # Update params summary on slider changes
        for slider in [
            self.temperature_slider,
            self.max_tokens_slider,
            self.top_p_slider,
            self.top_k_slider,
        ]:
            slider.change(
                fn=lambda *args: self._update_params_summary(),
                inputs=[],
                outputs=[self.params_summary],
            )
        
        # Apply button
        self.apply_btn.click(
            fn=self._apply_parameters,
            inputs=[],
            outputs=[self.params_summary],
        )
        
        # Reset button
        self.reset_btn.click(
            fn=self._reset_to_defaults,
            inputs=[],
            outputs=[
                self.preset_dropdown,
                self.temperature_slider, self.temperature_num,
                self.max_tokens_slider, self.max_tokens_num,
                self.top_p_slider, self.top_p_num,
                self.top_k_slider, self.top_k_num,
                self.preset_description,
                self.params_summary,
            ],
        )
    
    def _apply_preset(self, preset_name):
        """Apply a preset configuration."""
        preset = self.PRESETS[preset_name]
        
        self.current_params = {
            "temperature": preset["temperature"],
            "max_tokens": preset["max_tokens"],
            "top_p": preset["top_p"],
            "top_k": preset["top_k"],
        }
        
        return [
            gr.update(value=preset["temperature"]),
            gr.update(value=preset["temperature"]),
            gr.update(value=preset["max_tokens"]),
            gr.update(value=preset["max_tokens"]),
            gr.update(value=preset["top_p"]),
            gr.update(value=preset["top_p"]),
            gr.update(value=preset["top_k"]),
            gr.update(value=preset["top_k"]),
            gr.update(value=preset["description"]),
            gr.update(value=self._get_current_params()),
        ]
    
    def _get_current_params(self) -> Dict[str, Any]:
        """Get current parameter values."""
        return {
            "temperature": self.temperature_slider.value,
            "max_tokens": int(self.max_tokens_slider.value),
            "top_p": self.top_p_slider.value,
            "top_k": int(self.top_k_slider.value),
            "repetition_penalty": self.repetition_penalty.value,
            "seed": self.seed_input.value if self.seed_checkbox.value else None,
        }
    
    def _update_params_summary(self):
        """Update the parameters summary display."""
        return gr.update(value=self._get_current_params())
    
    def _apply_parameters(self):
        """Apply current parameters to the engine."""
        params = self._get_current_params()
        
        if self.on_parameter_change:
            self.on_parameter_change(**params)
        
        return gr.update(value=params)
    
    def _reset_to_defaults(self):
        """Reset all parameters to default values."""
        default_preset = "⚖️ Balanced"
        preset = self.PRESETS[default_preset]
        
        return [
            gr.update(value=default_preset),
            gr.update(value=preset["temperature"]),
            gr.update(value=preset["temperature"]),
            gr.update(value=preset["max_tokens"]),
            gr.update(value=preset["max_tokens"]),
            gr.update(value=preset["top_p"]),
            gr.update(value=preset["top_p"]),
            gr.update(value=preset["top_k"]),
            gr.update(value=preset["top_k"]),
            gr.update(value=preset["description"]),
            gr.update(value=preset),
        ]
    
    def get_parameters(self) -> Dict[str, Any]:
        """Get the current parameter configuration."""
        return self._get_current_params()
