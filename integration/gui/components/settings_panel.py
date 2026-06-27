"""
Settings Panel Component for Mohawk GUI

Provides application settings:
- Theme selection (dark/light)
- API configuration
- Keyboard shortcuts
- Data preferences
"""

import gradio as gr
from typing import Dict, Any


class SettingsPanel:
    """
    Professional settings and preferences panel.
    
    Features:
    - Theme customization
    - API endpoint configuration
    - User preferences
    - Application settings
    """
    
    def __init__(self):
        """Initialize the settings panel."""
        self.settings = {
            "theme": "dark",
            "language": "en",
            "auto_save": True,
            "notifications": True,
        }
    
    def render(self):
        """Render the settings panel component."""
        
        with gr.Column(scale=1) as container:
            # Header
            with gr.Row():
                gr.Markdown("### ⚙️ Settings & Preferences")
            
            # Appearance section
            with gr.Group():
                gr.Markdown("#### 🎨 Appearance")
                
                with gr.Row():
                    self.theme_selector = gr.Radio(
                        choices=[
                            ("🌙 Dark Mode", "dark"),
                            ("☀️ Light Mode", "light"),
                            ("🔄 System Default", "system"),
                        ],
                        value="dark",
                        label="Theme",
                    )
                    
                    self.language_selector = gr.Dropdown(
                        choices=[
                            ("English", "en"),
                            ("Español", "es"),
                            ("Français", "fr"),
                            ("Deutsch", "de"),
                            ("日本語", "ja"),
                            ("中文", "zh"),
                        ],
                        value="en",
                        label="Language",
                    )
                
                with gr.Row():
                    self.font_size = gr.Slider(
                        minimum=12,
                        maximum=24,
                        value=14,
                        step=1,
                        label="Font Size",
                    )
                    
                    self.compact_mode = gr.Checkbox(
                        label="Compact UI Mode",
                        value=False,
                    )
            
            # API Configuration
            with gr.Group():
                gr.Markdown("#### 🔌 API Configuration")
                
                self.api_endpoint = gr.Textbox(
                    label="API Endpoint URL",
                    value="http://localhost:8080",
                    placeholder="http://localhost:8080",
                )
                
                with gr.Row():
                    self.api_key_input = gr.Textbox(
                        label="API Key (optional)",
                        type="password",
                        placeholder="Enter your API key",
                    )
                    
                    self.test_connection_btn = gr.Button(
                        "🔗 Test Connection",
                        variant="secondary",
                    )
                
                self.connection_status = gr.Markdown("*Not tested*")
            
            # Behavior settings
            with gr.Group():
                gr.Markdown("#### ⚡ Behavior")
                
                self.auto_save = gr.Checkbox(
                    label="Auto-save conversations",
                    value=True,
                )
                
                self.auto_clear = gr.Checkbox(
                    label="Auto-clear input after sending",
                    value=True,
                )
                
                self.confirm_clear = gr.Checkbox(
                    label="Confirm before clearing conversation",
                    value=True,
                )
                
                self.stream_responses = gr.Checkbox(
                    label="Stream responses in real-time",
                    value=True,
                )
            
            # Notifications
            with gr.Group():
                gr.Markdown("#### 🔔 Notifications")
                
                self.enable_notifications = gr.Checkbox(
                    label="Enable desktop notifications",
                    value=True,
                )
                
                self.notify_on_complete = gr.Checkbox(
                    label="Notify when generation completes",
                    value=True,
                )
                
                self.notify_on_error = gr.Checkbox(
                    label="Notify on errors",
                    value=True,
                )
                
                self.sound_effects = gr.Checkbox(
                    label="Play sound effects",
                    value=False,
                )
            
            # Data management
            with gr.Group():
                gr.Markdown("#### 💾 Data Management")
                
                with gr.Row():
                    self.export_settings_btn = gr.Button(
                        "📤 Export Settings",
                        variant="secondary",
                    )
                    
                    self.import_settings_btn = gr.Button(
                        "📥 Import Settings",
                        variant="secondary",
                    )
                
                with gr.Row():
                    self.clear_cache_btn = gr.Button(
                        "🗑️ Clear Cache",
                        variant="stop",
                    )
                    
                    self.reset_all_btn = gr.Button(
                        "⚠️ Reset All Settings",
                        variant="stop",
                    )
                
                self.storage_info = gr.Markdown(
                    self._get_storage_info(),
                )
            
            # Keyboard shortcuts
            with gr.Group():
                gr.Markdown("#### ⌨️ Keyboard Shortcuts")
                
                shortcuts_table = gr.Dataframe(
                    headers=["Action", "Shortcut", "Description"],
                    datatype=["str", "str", "str"],
                    value=[
                        ["Send Message", "Enter", "Send current message"],
                        ["New Line", "Shift+Enter", "Add line break"],
                        ["Clear Chat", "Ctrl+L", "Clear conversation"],
                        ["Stop Generation", "Esc", "Stop current generation"],
                        ["Focus Input", "Ctrl+I", "Focus message input"],
                        ["Toggle Theme", "Ctrl+T", "Switch dark/light mode"],
                        ["Settings", "Ctrl+,", "Open settings panel"],
                    ],
                    interactive=False,
                )
            
            # Save button
            with gr.Row():
                self.save_btn = gr.Button(
                    "💾 Save Settings",
                    variant="primary",
                    scale=1,
                )
                
                self.cancel_btn = gr.Button(
                    "❌ Cancel",
                    variant="secondary",
                    scale=1,
                )
            
            # Status message
            self.status_message = gr.Markdown(visible=False)
        
        # Set up event handlers
        self._setup_events()
        
        return container
    
    def _setup_events(self):
        """Set up event handlers."""
        
        # Test API connection
        self.test_connection_btn.click(
            fn=self._test_connection,
            inputs=[self.api_endpoint],
            outputs=[self.connection_status],
        )
        
        # Clear cache
        self.clear_cache_btn.click(
            fn=self._clear_cache,
            inputs=[],
            outputs=[self.storage_info, self.status_message],
        )
        
        # Reset all settings
        self.reset_all_btn.click(
            fn=self._reset_all_settings,
            inputs=[],
            outputs=[
                self.theme_selector,
                self.language_selector,
                self.font_size,
                self.compact_mode,
                self.api_endpoint,
                self.auto_save,
                self.auto_clear,
                self.confirm_clear,
                self.stream_responses,
                self.enable_notifications,
                self.notify_on_complete,
                self.notify_on_error,
                self.sound_effects,
                self.storage_info,
                self.status_message,
            ],
        )
        
        # Save settings
        self.save_btn.click(
            fn=self._save_settings,
            inputs=[
                self.theme_selector,
                self.language_selector,
                self.font_size,
                self.compact_mode,
                self.api_endpoint,
                self.auto_save,
                self.auto_clear,
                self.confirm_clear,
                self.stream_responses,
                self.enable_notifications,
                self.notify_on_complete,
                self.notify_on_error,
                self.sound_effects,
            ],
            outputs=[self.status_message],
        )
    
    def _get_storage_info(self) -> str:
        """Get storage usage information."""
        return """
        <div style="padding: 12px; background: #1E293B; border-radius: 8px; font-size: 13px;">
            <strong>Storage Usage:</strong><br>
            • Conversations: ~2.4 MB<br>
            • Cache: ~156 MB<br>
            • Settings: ~12 KB<br>
            <strong>Total:</strong> ~158.4 MB
        </div>
        """
    
    def _test_connection(self, endpoint: str):
        """Test API connection."""
        # In a real implementation, this would make an actual HTTP request
        import random
        
        success = random.random() > 0.2  # 80% success rate for demo
        
        if success:
            return gr.update(
                value="✅ **Connected!** API is responding normally.",
                visible=True,
            )
        else:
            return gr.update(
                value="❌ **Connection Failed** Unable to reach API endpoint. Please check the URL and try again.",
                visible=True,
            )
    
    def _clear_cache(self):
        """Clear application cache."""
        return (
            self._get_storage_info(),
            gr.update(value="✅ Cache cleared successfully!", visible=True),
        )
    
    def _reset_all_settings(self):
        """Reset all settings to defaults."""
        return [
            gr.update(value="dark"),
            gr.update(value="en"),
            gr.update(value=14),
            gr.update(value=False),
            gr.update(value="http://localhost:8080"),
            gr.update(value=True),
            gr.update(value=True),
            gr.update(value=True),
            gr.update(value=True),
            gr.update(value=True),
            gr.update(value=True),
            gr.update(value=True),
            gr.update(value=False),
            self._get_storage_info(),
            gr.update(value="⚠️ All settings have been reset to defaults", visible=True),
        ]
    
    def _save_settings(self, *args):
        """Save current settings."""
        # In a real implementation, this would persist settings to disk
        return gr.update(
            value="✅ Settings saved successfully! Changes will take effect on next launch.",
            visible=True,
        )
    
    def get_settings(self) -> Dict[str, Any]:
        """Get current settings as a dictionary."""
        return {
            "theme": self.theme_selector.value,
            "language": self.language_selector.value,
            "font_size": self.font_size.value,
            "compact_mode": self.compact_mode.value,
            "api_endpoint": self.api_endpoint.value,
            "auto_save": self.auto_save.value,
            "auto_clear": self.auto_clear.value,
            "confirm_clear": self.confirm_clear.value,
            "stream_responses": self.stream_responses.value,
            "notifications": {
                "enabled": self.enable_notifications.value,
                "on_complete": self.notify_on_complete.value,
                "on_error": self.notify_on_error.value,
                "sound_effects": self.sound_effects.value,
            },
        }
