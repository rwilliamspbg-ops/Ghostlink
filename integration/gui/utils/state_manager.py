"""
State Manager for Mohawk GUI

Manages application state across components and sessions.
"""

import json
from typing import Dict, Any, Optional
from pathlib import Path


class StateManager:
    """
    Manages application state for the GUI.
    
    Features:
    - Persistent settings storage
    - Session management
    - State synchronization
    - Auto-save functionality
    """
    
    def __init__(self, storage_path: Optional[str] = None):
        """
        Initialize the state manager.
        
        Args:
            storage_path: Path to store persistent state (optional)
        """
        self.storage_path = Path(storage_path) if storage_path else None
        self.session_state: Dict[str, Any] = {}
        self.persistent_state: Dict[str, Any] = {}
        
        # Load persistent state if available
        if self.storage_path and self.storage_path.exists():
            self._load_state()
    
    def get(self, key: str, default: Any = None) -> Any:
        """Get a value from session state."""
        return self.session_state.get(key, default)
    
    def set(self, key: str, value: Any):
        """Set a value in session state."""
        self.session_state[key] = value
    
    def get_persistent(self, key: str, default: Any = None) -> Any:
        """Get a value from persistent state."""
        return self.persistent_state.get(key, default)
    
    def set_persistent(self, key: str, value: Any, auto_save: bool = True):
        """
        Set a value in persistent state.
        
        Args:
            key: State key
            value: State value
            auto_save: If True, save to disk immediately
        """
        self.persistent_state[key] = value
        
        if auto_save and self.storage_path:
            self._save_state()
    
    def _load_state(self):
        """Load state from disk."""
        try:
            with open(self.storage_path, 'r') as f:
                self.persistent_state = json.load(f)
        except Exception as e:
            print(f"Warning: Could not load state: {e}")
            self.persistent_state = {}
    
    def _save_state(self):
        """Save state to disk."""
        try:
            # Ensure directory exists
            self.storage_path.parent.mkdir(parents=True, exist_ok=True)
            
            with open(self.storage_path, 'w') as f:
                json.dump(self.persistent_state, f, indent=2, default=str)
        except Exception as e:
            print(f"Warning: Could not save state: {e}")
    
    def clear_session(self):
        """Clear session state."""
        self.session_state = {}
    
    def reset_all(self):
        """Reset all state (session and persistent)."""
        self.clear_session()
        self.persistent_state = {}
        
        if self.storage_path and self.storage_path.exists():
            self.storage_path.unlink()
    
    def export_state(self) -> Dict[str, Any]:
        """Export all state as a dictionary."""
        return {
            "session": self.session_state.copy(),
            "persistent": self.persistent_state.copy(),
        }
    
    def import_state(self, state: Dict[str, Any]):
        """Import state from a dictionary."""
        if "session" in state:
            self.session_state = state["session"]
        if "persistent" in state:
            self.persistent_state = state["persistent"]
    
    # Convenience methods for common state items
    
    @property
    def current_model(self) -> Optional[str]:
        """Get the currently loaded model."""
        return self.get("current_model")
    
    @current_model.setter
    def current_model(self, value: str):
        """Set the currently loaded model."""
        self.set("current_model", value)
    
    @property
    def theme(self) -> str:
        """Get the current theme."""
        return self.get_persistent("theme", "dark")
    
    @theme.setter
    def theme(self, value: str):
        """Set the theme."""
        self.set_persistent("theme", value)
    
    @property
    def generation_params(self) -> Dict[str, Any]:
        """Get current generation parameters."""
        return self.get_persistent("generation_params", {
            "temperature": 0.7,
            "max_tokens": 512,
            "top_p": 0.9,
            "top_k": 40,
        })
    
    @generation_params.setter
    def generation_params(self, value: Dict[str, Any]):
        """Set generation parameters."""
        self.set_persistent("generation_params", value)
    
    @property
    def conversation_history(self) -> list:
        """Get conversation history."""
        return self.get("conversation_history", [])
    
    @conversation_history.setter
    def conversation_history(self, value: list):
        """Set conversation history."""
        self.set("conversation_history", value)
