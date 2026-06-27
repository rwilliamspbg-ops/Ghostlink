"""
Main GUI Application for Mohawk Inference Engine

Provides the main Gradio application that integrates all components
into a cohesive, professional user interface.
"""

import gradio as gr
from typing import Optional
from ..api.server import APIServer
from .components import (
    ChatInterface,
    ModelManager,
    ParameterPanel,
    MetricsDashboard,
    SettingsPanel,
)
from .styles.theme import get_theme, CUSTOM_CSS


def create_gui_app(
    server: Optional[APIServer] = None,
    title: str = "Mohawk Inference Engine",
    share: bool = False,
) -> gr.Blocks:
    """
    Create the main Gradio application for Mohawk.
    
    Args:
        server: APIServer instance with inference engine
        title: Application title
        share: If True, create a public shareable link
        
    Returns:
        Configured Gradio Blocks application
    """
    
    # Create custom theme
    theme = get_theme(dark_mode=True)
    
    # Build the application
    with gr.Blocks(
        title=title,
        theme=theme,
        css=CUSTOM_CSS,
        fill_height=True,
    ) as app:
        
        # Header
        with gr.Row(elem_classes=["header-row"]):
            with gr.Column(scale=1):
                gr.Markdown(
                    "# 🦅 Mohawk Inference Engine",
                    elem_classes=["main-title"],
                )
                gr.Markdown(
                    "*High-performance local LLM inference with a beautiful interface*",
                    elem_classes=["subtitle"],
                )
            
            with gr.Column(scale=0, min_width=200):
                # Quick status indicator
                status_indicator = gr.HTML(
                    _get_status_badge_html(server),
                    elem_classes=["status-badge"],
                )
        
        # Main navigation tabs
        with gr.Tabs(selected=0) as tabs:
            
            # 💬 Chat Tab
            with gr.TabItem("💬 Chat", id="chat"):
                chat_interface = ChatInterface(server)
                chat_interface.render()
            
            # 📁 Models Tab
            with gr.TabItem("📁 Models", id="models"):
                model_manager = ModelManager(server)
                model_manager.render()
            
            # ⚙️ Parameters Tab
            with gr.TabItem("⚙️ Parameters", id="parameters"):
                param_panel = ParameterPanel()
                param_panel.render()
            
            # 📊 Metrics Tab
            with gr.TabItem("📊 Metrics", id="metrics"):
                metrics_dashboard = MetricsDashboard(server)
                metrics_dashboard.render()
            
            # ⚙️ Settings Tab
            with gr.TabItem("⚙️ Settings", id="settings"):
                settings_panel = SettingsPanel()
                settings_panel.render()
        
        # Footer
        with gr.Row(elem_classes=["footer-row"]):
            gr.Markdown(
                f"""
                <div style="text-align: center; padding: 20px; color: #94A3B8; font-size: 13px;">
                    <strong>Mohawk Inference Engine</strong> v0.1.0 | 
                    Built with ❤️ for high-performance local AI | 
                    <a href="/docs" target="_blank">API Documentation</a>
                </div>
                """,
            )
        
        # Store component references in app state
        app.chat_interface = chat_interface
        app.model_manager = model_manager
        app.param_panel = param_panel
        app.metrics_dashboard = metrics_dashboard
        app.settings_panel = settings_panel
    
    return app


def _get_status_badge_html(server: Optional[APIServer]) -> str:
    """Generate HTML for the status badge."""
    if server and server.engine.is_loaded:
        return """
        <div style="
            display: inline-flex;
            align-items: center;
            padding: 8px 16px;
            background: linear-gradient(135deg, #10B981 0%, #059669 100%);
            border-radius: 20px;
            color: white;
            font-weight: 500;
            font-size: 13px;
        ">
            <span style="
                width: 8px;
                height: 8px;
                background: white;
                border-radius: 50%;
                margin-right: 8px;
                animation: pulse 2s infinite;
            "></span>
            Model Ready
        </div>
        """
    else:
        return """
        <div style="
            display: inline-flex;
            align-items: center;
            padding: 8px 16px;
            background: linear-gradient(135deg, #64748B 0%, #475569 100%);
            border-radius: 20px;
            color: white;
            font-weight: 500;
            font-size: 13px;
        ">
            <span style="
                width: 8px;
                height: 8px;
                background: rgba(255,255,255,0.5);
                border-radius: 50%;
                margin-right: 8px;
            "></span>
            No Model Loaded
        </div>
        """


def launch_gui(
    server: Optional[APIServer] = None,
    host: str = "127.0.0.1",
    port: int = 7860,
    share: bool = False,
    inbrowser: bool = True,
):
    """
    Launch the GUI application.
    
    Args:
        server: APIServer instance
        host: Host to bind to
        port: Port to listen on
        share: Create public shareable link
        inbrowser: Open browser automatically
    """
    app = create_gui_app(server, share=share)
    app.launch(
        server_name=host,
        server_port=port,
        share=share,
        inbrowser=inbrowser,
    )


# CLI entry point
def main():
    """Main entry point for running the GUI standalone."""
    import argparse
    from ..engine import InferenceEngine
    from ..api.server import APIServer
    
    parser = argparse.ArgumentParser(description="Launch Mohawk GUI")
    parser.add_argument("--host", default="127.0.0.1", help="Host to bind to")
    parser.add_argument("--port", type=int, default=7860, help="Port for GUI")
    parser.add_argument("--api-port", type=int, default=8080, help="Port for API server")
    parser.add_argument("--model", default=None, help="Model to load on startup")
    parser.add_argument("--device", default="cpu", help="Device for inference")
    parser.add_argument("--share", action="store_true", help="Create public link")
    parser.add_argument("--no-browser", action="store_true", help="Don't open browser")
    
    args = parser.parse_args()
    
    # Initialize engine and server
    engine = InferenceEngine(device=args.device)
    if args.model:
        engine.load_model(args.model)
    
    server = APIServer(engine=engine, port=args.api_port)
    
    # Launch GUI
    print(f"🦅 Launching Mohawk GUI at http://{args.host}:{args.port}")
    print(f"📡 API Server running on http://{args.host}:{args.api_port}")
    print("Press Ctrl+C to stop\n")
    
    launch_gui(
        server=server,
        host=args.host,
        port=args.port,
        share=args.share,
        inbrowser=not args.no_browser,
    )


if __name__ == "__main__":
    main()
