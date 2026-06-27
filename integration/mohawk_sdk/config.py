"""
Configuration management for Mohawk Inference Engine.

Handles worker discovery, PQC settings, session policies, and telemetry.
"""

import tomli
from pathlib import Path
from typing import Optional, Dict, Any, List
import json


class MohawkConfig:
    """
    Configuration manager for Mohawk Inference Engine.
    
    Handles:
    - Worker discovery and registration
    - PQC key management
    - Session policies
    - Telemetry settings
    
    Example usage:
        >>> config = MohawkConfig()
        >>> config.set_pqc_enabled(True)
        >>> config.save()
    """
    
    DEFAULT_CONFIG_PATH = Path("~/.mohawk/config.toml")
    
    def __init__(self, config_path: Optional[Path] = None):
        self.config_path = Path(config_path) if config_path else self.DEFAULT_CONFIG_PATH
        self._config: Dict[str, Any] = {}
        self.load()
    
    def load(self):
        """Load configuration from file."""
        if not self.config_path.exists():
            self._config = self.get_default_config()
            return
        
        with open(self.config_path, "rb") as f:
            self._config = tomli.load(f)
    
    def save(self):
        """Save configuration to file."""
        self.config_path.parent.mkdir(parents=True, exist_ok=True)
        
        # Convert to JSON for now (tomli doesn't write nicely)
        with open(self.config_path, "w") as f:
            json.dump(self._config, f, indent=2)
    
    def get_default_config(self) -> Dict[str, Any]:
        """Get default configuration."""
        return {
            "worker": {
                "host": "localhost",
                "port": 8003,
            },
            "security": {
                "pqc_enabled": True,
                "replay_protection": True,
                "nonce_expiry_seconds": 3600,
            },
            "session": {
                "max_concurrent_sessions": 100,
                "circuit_breaker_threshold": 5,
                "circuit_breaker_timeout": 30,
            },
            "telemetry": {
                "enabled": True,
                "metrics_endpoint": "http://localhost:9090",
            },
        }
    
    def set_pqc_enabled(self, enabled: bool):
        """Enable/disable PQC encryption."""
        self._config["security"]["pqc_enabled"] = enabled
        self.save()
    
    def set_max_concurrent_sessions(self, limit: int):
        """Set maximum concurrent sessions."""
        self._config["session"]["max_concurrent_sessions"] = limit
        self.save()
    
    def get_worker_url(self) -> str:
        """Get worker URL."""
        return f"http://{self._config['worker']['host']}:{self._config['worker']['port']}"
    
    def discover_workers(self, timeout: float = 5.0) -> List[Dict]:
        """
        Discover available workers on the network.
        
        Args:
            timeout: Request timeout in seconds
            
        Returns:
            List of worker information dictionaries
        """
        # For now, return empty list
        # In production, this would scan network for workers
        return []
    
    def register_worker(self, worker_info: Dict[str, Any]):
        """Register a new worker with the controller."""
        # Placeholder - implementation would call controller API
        print(f"Would register worker: {worker_info}")
    
    def get(self, key: str) -> Optional[Any]:
        """Get configuration value by key path."""
        keys = key.split(".")
        value = self._config
        
        for k in keys:
            if isinstance(value, dict) and k in value:
                value = value[k]
            else:
                return None
        
        return value
    
    def set(self, key: str, value: Any):
        """Set configuration value by key path."""
        keys = key.split(".")
        parent_key = ".".join(keys[:-1])
        leaf_key = keys[-1]
        
        if parent_key:
            parent = self.get(parent_key)
            if isinstance(parent, dict) and leaf_key in parent:
                parent[leaf_key] = value
                self.save()
    
    def __repr__(self):
        return f"MohawkConfig(path={self.config_path}, pqc_enabled={self._config.get('security', {}).get('pqc_enabled', True)})"


def load_config(config_path: Optional[str | Path] = None) -> MohawkConfig:
    """
    Factory function to load configuration.
    
    Args:
        config_path: Path to configuration file (optional)
        
    Returns:
        Configured MohawkConfig instance
    """
    return MohawkConfig(config_path=config_path)


def create_default_config() -> MohawkConfig:
    """
    Create a new default configuration.
    
    Returns:
        New MohawkConfig with default settings
    """
    config = MohawkConfig()
    return config
