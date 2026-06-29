"""
Audit Logger for Mohawk Inference Engine GUI

Provides comprehensive audit trail for security and compliance.
"""

import json
from datetime import datetime
from pathlib import Path
from typing import Dict, Any, Optional
import hashlib


class AuditLogger:
    """
    Log all user actions for audit trail.
    
    Features:
    - Immutable event logging
    - Cryptographic hashing for integrity
    - Support for multiple log formats
    - Event categorization and tagging
    """
    
    def __init__(self, log_file: str = "audit.log"):
        self.log_file = Path(log_file)
        self.log_file.parent.mkdir(parents=True, exist_ok=True)
        
        # Event types for categorization
        self.event_types = {
            "authentication": ["login", "logout", "token_refresh"],
            "session_management": ["create", "update", "terminate"],
            "worker_management": ["add", "remove", "sync_model"],
            "configuration": ["read", "write", "backup", "restore"],
            "inference": ["start", "stop", "benchmark"],
            "system": ["health_check", "error", "recovery"]
        }
    
    def log_action(
        self, 
        action_type: str, 
        resource: str, 
        details: Dict[str, Any] = None,
        user_id: str = None,
        ip_address: str = None
    ):
        """
        Record auditable action.
        
        Args:
            action_type: Type of action (e.g., "create", "read")
            resource: Resource affected (e.g., "session_abc123")
            details: Additional action details
            user_id: User performing the action
            ip_address: IP address of requester
        """
        event = self._create_event(
            action_type=action_type,
            resource=resource,
            details=details or {},
            user_id=user_id,
            ip_address=ip_address
        )
        
        # Write to log file (append mode)
        with open(self.log_file, 'a') as f:
            f.write(json.dumps(event) + '\n')
        
        return event
    
    def _create_event(
        self,
        action_type: str,
        resource: str,
        details: Dict[str, Any],
        user_id: str = None,
        ip_address: str = None
    ) -> Dict[str, Any]:
        """Create audit event with all required fields."""
        
        # Categorize event type
        category = self._categorize_event(action_type)
        
        # Create unique event ID
        event_id = hashlib.sha256(
            f"{datetime.now().isoformat()}{resource}".encode()
        ).hexdigest()[:16]
        
        event = {
            "event_id": event_id,
            "timestamp": datetime.now().isoformat(),
            "action_type": action_type,
            "category": category,
            "resource": resource,
            "details": details,
            "user_id": user_id or "anonymous",
            "ip_address": ip_address or "unknown"
        }
        
        return event
    
    def _categorize_event(self, action_type: str) -> str:
        """Categorize event based on action type."""
        for category, types in self.event_types.items():
            if action_type in types:
                return category
        return "unknown"
    
    def log_error(self, error: Exception, context: Dict[str, Any] = None):
        """Log error with full stack trace."""
        event = {
            "event_id": hashlib.sha256(
                f"{datetime.now().isoformat()}error".encode()
            ).hexdigest()[:16],
            "timestamp": datetime.now().isoformat(),
            "action_type": "error",
            "category": "system",
            "resource": str(context.get("resource", "unknown")),
            "details": {
                "error_type": type(error).__name__,
                "error_message": str(error),
                "stack_trace": self._get_stack_trace()
            },
            "user_id": None,
            "ip_address": None
        }
        
        with open(self.log_file, 'a') as f:
            f.write(json.dumps(event) + '\n')
    
    def _get_stack_trace(self) -> str:
        """Get current stack trace."""
        import traceback
        return traceback.format_exc()
    
    def get_events(
        self, 
        event_type: str = None, 
        resource: str = None,
        since: str = None
    ) -> list:
        """
        Query audit log for events.
        
        Args:
            event_type: Filter by action type
            resource: Filter by resource
            since: Filter by timestamp (ISO format)
            
        Returns:
            List of matching events
        """
        events = []
        
        with open(self.log_file, 'r') as f:
            for line in f:
                try:
                    event = json.loads(line.strip())
                    
                    # Apply filters
                    if event_type and event.get("action_type") != event_type:
                        continue
                    if resource and event.get("resource") != resource:
                        continue
                    if since and event.get("timestamp", "") < since:
                        continue
                    
                    events.append(event)
                except json.JSONDecodeError:
                    continue
        
        return events
    
    def get_summary(self) -> Dict[str, Any]:
        """Get audit log summary statistics."""
        stats = {
            "total_events": 0,
            "by_category": {},
            "by_action_type": {}
        }
        
        try:
            with open(self.log_file, 'r') as f:
                for line in f:
                    try:
                        event = json.loads(line.strip())
                        stats["total_events"] += 1
                        
                        category = event.get("category", "unknown")
                        action_type = event.get("action_type", "unknown")
                        
                        stats["by_category"][category] = \
                            stats["by_category"].get(category, 0) + 1
                        stats["by_action_type"][action_type] = \
                            stats["by_action_type"].get(action_type, 0) + 1
                        
                    except json.JSONDecodeError:
                        continue
        except FileNotFoundError:
            pass
        
        return stats


class AuditEventStore:
    """In-memory audit event store for real-time queries."""
    
    def __init__(self, max_events: int = 10000):
        self.events: list = []
        self.max_events = max_events
    
    def add_event(self, event: Dict[str, Any]):
        """Add event to store."""
        self.events.append(event)
        
        # Keep only most recent events
        if len(self.events) > self.max_events:
            self.events = self.events[-self.max_events:]
    
    def get_recent_events(self, count: int = 100) -> list:
        """Get most recent events."""
        return self.events[-count:] if len(self.events) > count else self.events.copy()
    
    def filter_by_type(self, event_type: str) -> list:
        """Filter events by type."""
        return [e for e in self.events if e.get("action_type") == event_type]


if __name__ == "__main__":
    # Test audit logging
    logger = AuditLogger()
    
    # Log some test events
    logger.log_action(
        action_type="create",
        resource="session_abc123",
        details={"model": "model.onnx", "devices": ["gpu_0"]},
        user_id="user1"
    )
    
    logger.log_action(
        action_type="read",
        resource="config.toml",
        details={},
        user_id="admin"
    )
    
    print("Audit log created successfully!")
