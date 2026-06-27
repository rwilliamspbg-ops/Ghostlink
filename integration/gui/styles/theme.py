"""
Custom theme configuration for Mohawk GUI

Provides a professional, modern color scheme with dark/light mode support.
"""

import gradio as gr


# Professional color palette
COLORS = {
    # Primary (Indigo)
    "primary": "#6366F1",
    "primary_hover": "#4F46E5",
    "primary_light": "#A5B4FC",
    
    # Secondary (Emerald)
    "secondary": "#10B981",
    "secondary_hover": "#059669",
    
    # Accent colors
    "accent": "#F59E0B",      # Amber for warnings
    "danger": "#EF4444",       # Red for errors
    "info": "#3B82F6",         # Blue for info
    
    # Dark mode
    "bg_dark": "#0F172A",      # Slate 900
    "surface_dark": "#1E293B", # Slate 800
    "surface_dark_light": "#334155",
    
    # Light mode
    "bg_light": "#F8FAFC",     # Slate 50
    "surface_light": "#FFFFFF",
    "surface_light_border": "#E2E8F0",
    
    # Text
    "text_primary_dark": "#F8FAFC",
    "text_secondary_dark": "#94A3B8",
    "text_primary_light": "#0F172A",
    "text_secondary_light": "#64748B",
}


CUSTOM_CSS = """
/* Mohawk Custom Styles */

:root {
    --mohawk-primary: #6366F1;
    --mohawk-primary-hover: #4F46E5;
    --mohawk-secondary: #10B981;
    --mohawk-accent: #F59E0B;
    --mohawk-danger: #EF4444;
}

/* Smooth transitions */
.gradio-container, .gr-button, .gr-input, .gr-dropdown, .gr-slider {
    transition: all 0.2s ease-in-out !important;
}

/* Button hover effects */
.gr-button:hover {
    transform: translateY(-1px);
    box-shadow: 0 4px 12px rgba(99, 102, 241, 0.3) !important;
}

.gr-button:active {
    transform: translateY(0);
}

/* Chat message styling */
.chat-message-user {
    background: linear-gradient(135deg, #6366F1 0%, #4F46E5 100%) !important;
    border-radius: 12px !important;
    padding: 12px 16px !important;
}

.chat-message-assistant {
    background: #1E293B !important;
    border-radius: 12px !important;
    padding: 12px 16px !important;
    border-left: 3px solid #10B981 !important;
}

/* Metric cards */
.metric-card {
    background: linear-gradient(135deg, #1E293B 0%, #0F172A 100%);
    border-radius: 16px;
    padding: 20px;
    border: 1px solid #334155;
}

/* Loading animation */
@keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
}

.loading-pulse {
    animation: pulse 1.5s ease-in-out infinite;
}

/* Scrollbar styling */
::-webkit-scrollbar {
    width: 8px;
    height: 8px;
}

::-webkit-scrollbar-track {
    background: #1E293B;
    border-radius: 4px;
}

::-webkit-scrollbar-thumb {
    background: #475569;
    border-radius: 4px;
}

::-webkit-scrollbar-thumb:hover {
    background: #6366F1;
}

/* Code block styling */
.code-block {
    background: #0F172A;
    border: 1px solid #334155;
    border-radius: 8px;
    font-family: 'JetBrains Mono', 'Fira Code', monospace;
}

/* Toast notifications */
.toast-success {
    background: #10B981 !important;
    color: white !important;
}

.toast-error {
    background: #EF4444 !important;
    color: white !important;
}

/* Tab styling */
.tab-nav button {
    border-radius: 8px 8px 0 0 !important;
    padding: 12px 24px !important;
    font-weight: 500 !important;
}

/* Slider track */
.slider-track {
    background: linear-gradient(90deg, #6366F1 0%, #10B981 100%);
}

/* Input focus state */
.gr-input:focus, .gr-textarea:focus {
    border-color: #6366F1 !important;
    box-shadow: 0 0 0 3px rgba(99, 102, 241, 0.1) !important;
}

/* Card hover effect */
.hover-card {
    cursor: pointer;
}

.hover-card:hover {
    transform: translateY(-2px);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.3);
}

/* Progress bar animation */
.progress-bar {
    background: linear-gradient(90deg, #6366F1 0%, #10B981 100%);
    border-radius: 4px;
    transition: width 0.3s ease;
}

/* Model status indicators */
.status-active {
    color: #10B981;
    font-weight: bold;
}

.status-loading {
    color: #F59E0B;
    font-weight: bold;
}

.status-error {
    color: #EF4444;
    font-weight: bold;
}
"""


def get_theme(dark_mode: bool = True) -> gr.themes.Base:
    """
    Create a custom Gradio theme for Mohawk.
    
    Args:
        dark_mode: If True, use dark theme; otherwise light theme
        
    Returns:
        Configured Gradio theme object
    """
    if dark_mode:
        base_theme = gr.themes.Base(
            primary_hue="indigo",
            secondary_hue="emerald",
            neutral_hue="slate",
        )
        
        theme = base_theme.set(
            # Colors
            body_background_fill="#0F172A",
            body_background_fill_dark="#0F172A",
            block_background_fill="#1E293B",
            block_background_fill_dark="#1E293B",
            block_label_background_fill="#334155",
            block_label_background_fill_dark="#334155",
            
            # Text
            body_text_color="#F8FAFC",
            body_text_color_dark="#F8FAFC",
            body_text_color_subdued="#94A3B8",
            body_text_color_subdued_dark="#94A3B8",
            
            # Borders
            block_label_border_color="#475569",
            block_label_border_color_dark="#475569",
            block_title_border_color="#475569",
            
            # Buttons
            button_primary_background_fill="#6366F1",
            button_primary_background_fill_dark="#6366F1",
            button_primary_background_fill_hover="#4F46E5",
            button_primary_background_fill_hover_dark="#4F46E5",
            button_primary_text_color="white",
            button_primary_text_color_dark="white",
            
            button_secondary_background_fill="#10B981",
            button_secondary_background_fill_dark="#10B981",
            button_secondary_background_fill_hover="#059669",
            button_secondary_background_fill_hover_dark="#059669",
            
            # Inputs
            input_background_fill="#1E293B",
            input_background_fill_dark="#1E293B",
            input_border_color="#475569",
            input_border_color_dark="#475569",
            
            # Chatbot
            chatbot_code_background="#0F172A",
            chatbot_code_background_dark="#0F172A",
            
            # Spacing & sizing
            spacing_sm="4px",
            spacing_md="8px",
            spacing_lg="16px",
            spacing_xl="24px",
            
            radius_sm="4px",
            radius_md="8px",
            radius_lg="12px",
            radius_xl="16px",
            
            # Shadows
            shadow_drop="0 2px 8px rgba(0, 0, 0, 0.2)",
            shadow_drop_lg="0 4px 16px rgba(0, 0, 0, 0.3)",
            shadow_inset="inset 0 2px 4px rgba(0, 0, 0, 0.1)",
            
            # Font
            font_mono=['"JetBrains Mono"', '"Fira Code"', "monospace"],
            font_sans=['"Inter"', '"Segoe UI"', "sans-serif"],
        )
    else:
        base_theme = gr.themes.Base(
            primary_hue="indigo",
            secondary_hue="emerald",
            neutral_hue="slate",
        )
        
        theme = base_theme.set(
            # Colors - Light mode
            body_background_fill="#F8FAFC",
            body_background_fill_dark="#F8FAFC",
            block_background_fill="#FFFFFF",
            block_background_fill_dark="#FFFFFF",
            block_label_background_fill="#F1F5F9",
            block_label_background_fill_dark="#F1F5F9",
            
            # Text
            body_text_color="#0F172A",
            body_text_color_dark="#0F172A",
            body_text_color_subdued="#64748B",
            body_text_color_subdued_dark="#64748B",
            
            # Borders
            block_label_border_color="#E2E8F0",
            block_label_border_color_dark="#E2E8F0",
            
            # Buttons
            button_primary_background_fill="#6366F1",
            button_primary_background_fill_dark="#6366F1",
            button_primary_background_fill_hover="#4F46E5",
            button_primary_background_fill_hover_dark="#4F46E5",
            button_primary_text_color="white",
            button_primary_text_color_dark="white",
            
            button_secondary_background_fill="#10B981",
            button_secondary_background_fill_dark="#10B981",
            button_secondary_background_fill_hover="#059669",
            button_secondary_background_fill_hover_dark="#059669",
            
            # Inputs
            input_background_fill="#FFFFFF",
            input_background_fill_dark="#FFFFFF",
            input_border_color="#E2E8F0",
            input_border_color_dark="#E2E8F0",
            
            # Chatbot
            chatbot_code_background="#F1F5F9",
            chatbot_code_background_dark="#F1F5F9",
            
            # Spacing & sizing
            spacing_sm="4px",
            spacing_md="8px",
            spacing_lg="16px",
            spacing_xl="24px",
            
            radius_sm="4px",
            radius_md="8px",
            radius_lg="12px",
            radius_xl="16px",
            
            # Shadows
            shadow_drop="0 2px 8px rgba(0, 0, 0, 0.08)",
            shadow_drop_lg="0 4px 16px rgba(0, 0, 0, 0.12)",
            shadow_inset="inset 0 2px 4px rgba(0, 0, 0, 0.05)",
            
            # Font
            font_mono=['"JetBrains Mono"', '"Fira Code"', "monospace"],
            font_sans=['"Inter"', '"Segoe UI"', "sans-serif"],
        )
    
    return theme
