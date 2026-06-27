"""
Configuration management for Mohawk Inference Engine
"""

import os
from pathlib import Path
from typing import Optional, Dict, Any
from dataclasses import dataclass, field


@dataclass
class Config:
    """
    Configuration settings for the inference engine.
    
    Can be loaded from environment variables or config file.
    """
    
    # Server settings
    host: str = "0.0.0.0"
    port: int = 8080
    
    # Model settings
    model_path: Optional[str] = None
    default_max_tokens: int = 512
    default_temperature: float = 0.7
    
    # Performance settings
    num_threads: int = 4
    batch_size: int = 1
    
    # Cache settings
    cache_dir: str = field(default_factory=lambda: str(Path.home() / ".mohawk"))
    
    # Logging
    log_level: str = "INFO"
    log_file: Optional[str] = None
    
    @classmethod
    def from_env(cls) -> "Config":
        """Load configuration from environment variables"""
        return cls(
            host=os.getenv("MOHAWK_HOST", "0.0.0.0"),
            port=int(os.getenv("MOHAWK_PORT", "8080")),
            model_path=os.getenv("MOHAWK_MODEL_PATH"),
            default_max_tokens=int(os.getenv("MOHAWK_MAX_TOKENS", "512")),
            default_temperature=float(os.getenv("MOHAWK_TEMPERATURE", "0.7")),
            num_threads=int(os.getenv("MOHAWK_THREADS", "4")),
            log_level=os.getenv("MOHAWK_LOG_LEVEL", "INFO"),
            log_file=os.getenv("MOHAWK_LOG_FILE"),
        )
    
    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Config":
        """Load configuration from dictionary"""
        return cls(**data)
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert configuration to dictionary"""
        return {
            "host": self.host,
            "port": self.port,
            "model_path": self.model_path,
            "default_max_tokens": self.default_max_tokens,
            "default_temperature": self.default_temperature,
            "num_threads": self.num_threads,
            "batch_size": self.batch_size,
            "cache_dir": self.cache_dir,
            "log_level": self.log_level,
            "log_file": self.log_file,
        }
    
    def save(self, path: str) -> None:
        """Save configuration to JSON file"""
        import json
        
        with open(path, "w") as f:
            json.dump(self.to_dict(), f, indent=2)
    
    @classmethod
    def load(cls, path: str) -> "Config":
        """Load configuration from JSON file"""
        import json
        
        with open(path, "r") as f:
            data = json.load(f)
        
        return cls.from_dict(data)
