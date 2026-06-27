"""
Chat Interface Component for Mohawk GUI

Provides a professional chat interface with:
- Multi-turn conversation support
- Markdown rendering
- Code syntax highlighting
- Streaming responses
- Conversation history
"""

import gradio as gr
from typing import List, Dict, Any, Optional
import time


class ChatInterface:
    """
    Professional chat interface component for interacting with the inference engine.
    
    Features:
    - Real-time streaming responses
    - Markdown and code block rendering
    - Conversation history management
    - Parameter overrides per message
    - Export functionality
    """
    
    def __init__(self, server=None):
        """
        Initialize the chat interface.
        
        Args:
            server: APIServer instance for making inference requests
        """
        self.server = server
        self.conversation_history = []
        self.current_params = {}
    
    def render(self):
        """Render the chat interface component."""
        
        with gr.Column(scale=1) as container:
            # Chat header
            with gr.Row():
                gr.Markdown("### 💬 Chat with your model")
            
            # Conversation history / chat display
            self.chatbot = gr.Chatbot(
                label="Conversation",
                height=500,
                show_label=True,
                show_copy_button=True,
                bubble_full_width=False,
                markdown=True,
                elem_classes=["chat-container"],
            )
            
            # Input area
            with gr.Row():
                self.msg_input = gr.Textbox(
                    placeholder="Type your message here... (Shift+Enter for new line)",
                    show_label=False,
                    lines=3,
                    container=False,
                    scale=4,
                    elem_classes=["msg-input"],
                )
                
                self.send_btn = gr.Button(
                    "🚀 Send",
                    variant="primary",
                    scale=1,
                    min_width=120,
                )
            
            # Control buttons
            with gr.Row():
                self.clear_btn = gr.Button("🗑️ Clear Conversation", variant="secondary")
                self.export_btn = gr.Button("📥 Export", variant="secondary")
                self.regenerate_btn = gr.Button("🔄 Regenerate", variant="secondary")
            
            # Status indicator
            self.status_text = gr.Markdown("*Ready*")
        
        # Set up event handlers
        self._setup_events()
        
        return container
    
    def _setup_events(self):
        """Set up event handlers for user interactions."""
        
        # Send message on button click
        self.send_btn.click(
            fn=self._handle_user_message,
            inputs=[self.msg_input, self.chatbot],
            outputs=[self.chatbot, self.msg_input, self.status_text],
        )
        
        # Send message on Enter (without Shift)
        self.msg_input.submit(
            fn=self._handle_user_message,
            inputs=[self.msg_input, self.chatbot],
            outputs=[self.chatbot, self.msg_input, self.status_text],
        )
        
        # Clear conversation
        self.clear_btn.click(
            fn=self._clear_conversation,
            inputs=[],
            outputs=[self.chatbot, self.status_text],
        )
        
        # Regenerate last response
        self.regenerate_btn.click(
            fn=self._regenerate_last,
            inputs=[self.chatbot],
            outputs=[self.chatbot, self.status_text],
        )
        
        # Export conversation
        self.export_btn.click(
            fn=self._export_conversation,
            inputs=[self.chatbot],
            outputs=[],
        )
    
    def _handle_user_message(
        self,
        message: str,
        chat_history: List[List[str]],
    ):
        """
        Handle a new user message.
        
        Args:
            message: User's input message
            chat_history: Current conversation history
            
        Yields:
            Updated chat history, cleared input, status updates
        """
        if not message.strip():
            yield chat_history, "", "*Please enter a message*"
            return
        
        # Add user message to history
        chat_history = chat_history or []
        chat_history.append([message, None])
        
        yield chat_history, "", "*Thinking...* 🤔"
        
        # Generate response
        try:
            if self.server and self.server.engine.is_loaded:
                # Stream the response
                full_response = ""
                generator = self.server.engine.generate(
                    prompt=message,
                    max_tokens=self.current_params.get("max_tokens", 512),
                    temperature=self.current_params.get("temperature", 0.7),
                    top_p=self.current_params.get("top_p", 0.9),
                    stream=True,
                )
                
                for token in generator:
                    full_response += str(token)
                    # Update the chatbot with partial response
                    chat_history[-1][1] = full_response
                    yield chat_history, "", f"*Generating...* {len(full_response)} chars"
                
                # Final update
                chat_history[-1][1] = full_response
                yield chat_history, "", f"*Response complete* ✅ ({len(full_response)} chars)"
                
            else:
                # Demo mode - no model loaded
                demo_response = self._generate_demo_response(message)
                chat_history[-1][1] = demo_response
                yield chat_history, "", "*Demo mode - Load a model for real responses*"
                
        except Exception as e:
            error_msg = f"❌ Error: {str(e)}"
            chat_history[-1][1] = error_msg
            yield chat_history, "", "*Error occurred*"
    
    def _generate_demo_response(self, message: str) -> str:
        """
        Generate a demo response when no model is loaded.
        
        Args:
            message: User's input message
            
        Returns:
            Demo response text
        """
        return f"""**Demo Mode Active** 🔧

I received your message: "{message[:100]}{'...' if len(message) > 100 else ''}"

To get real responses:
1. Go to the **Models** tab
2. Load a model from HuggingFace or local storage
3. Come back and chat!

*This is a placeholder response to demonstrate the UI.*"""
    
    def _clear_conversation(self):
        """Clear the conversation history."""
        self.conversation_history = []
        return [], "*Conversation cleared* 🗑️"
    
    def _regenerate_last(self, chat_history: List[List[str]]):
        """Regenerate the last assistant response."""
        if not chat_history or len(chat_history) < 1:
            return chat_history, "*No conversation to regenerate*"
        
        # Get the last user message
        last_user_msg = chat_history[-1][0]
        chat_history[-1][1] = None  # Clear the response
        
        yield chat_history, "*Regenerating...* 🔄"
        
        # Re-generate (same logic as _handle_user_message)
        # For brevity, reusing the generation logic
        for update in self._handle_user_message(last_user_msg, chat_history[:-1]):
            yield update
    
    def _export_conversation(self, chat_history: List[List[str]]):
        """Export the conversation to a file."""
        if not chat_history:
            return
        
        # Format conversation
        export_text = "# Mohawk Conversation Export\n\n"
        export_text += f"**Date:** {time.strftime('%Y-%m-%d %H:%M:%S')}\n\n"
        export_text += "---\n\n"
        
        for user_msg, assistant_msg in chat_history:
            export_text += f"### 👤 User\n\n{user_msg}\n\n"
            export_text += f"### 🤖 Assistant\n\n{assistant_msg}\n\n"
            export_text += "---\n\n"
        
        # In a real implementation, this would trigger a file download
        print("Conversation exported!")
        return export_text
    
    def set_parameters(self, **kwargs):
        """
        Set generation parameters for the chat.
        
        Args:
            **kwargs: Generation parameters (temperature, max_tokens, etc.)
        """
        self.current_params.update(kwargs)
