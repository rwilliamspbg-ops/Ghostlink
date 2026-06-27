"""
WebSocket Handler for Mohawk GUI

Provides real-time bidirectional communication between
the GUI and the inference engine.
"""

import asyncio
import json
from typing import Optional, Dict, Any


class WebSocketHandler:
    """
    Handles WebSocket connections for real-time updates.
    
    Features:
    - Token streaming
    - Progress updates
    - Real-time metrics
    - Client management
    """
    
    def __init__(self):
        """Initialize the WebSocket handler."""
        self.connections = set()
        self.metrics_subscribers = set()
    
    async def connect(self, websocket):
        """Accept a new WebSocket connection."""
        await websocket.accept()
        self.connections.add(websocket)
        print(f"Client connected. Total connections: {len(self.connections)}")
    
    def disconnect(self, websocket):
        """Handle client disconnection."""
        self.connections.discard(websocket)
        self.metrics_subscribers.discard(websocket)
        print(f"Client disconnected. Total connections: {len(self.connections)}")
    
    async def send_token(self, token: str, session_id: str):
        """Send a generated token to the client."""
        message = {
            "type": "token",
            "session_id": session_id,
            "token": token,
        }
        await self.broadcast(message)
    
    async def send_metrics(self, metrics: Dict[str, Any]):
        """Send metrics update to subscribers."""
        message = {
            "type": "metrics",
            "data": metrics,
        }
        await self.broadcast_to_subscribers(message)
    
    async def broadcast(self, message: dict):
        """Broadcast a message to all connected clients."""
        if not self.connections:
            return
        
        message_json = json.dumps(message)
        
        # Send to all connections
        disconnected = set()
        for conn in self.connections:
            try:
                await conn.send_text(message_json)
            except Exception:
                disconnected.add(conn)
        
        # Clean up disconnected clients
        for conn in disconnected:
            self.disconnect(conn)
    
    async def broadcast_to_subscribers(self, message: dict):
        """Broadcast to metrics subscribers only."""
        if not self.metrics_subscribers:
            return
        
        message_json = json.dumps(message)
        
        disconnected = set()
        for conn in self.metrics_subscribers:
            try:
                await conn.send_text(message_json)
            except Exception:
                disconnected.add(conn)
        
        for conn in disconnected:
            self.disconnect(conn)
    
    def subscribe_to_metrics(self, websocket):
        """Subscribe a client to metrics updates."""
        self.metrics_subscribers.add(websocket)
    
    def unsubscribe_from_metrics(self, websocket):
        """Unsubscribe a client from metrics updates."""
        self.metrics_subscribers.discard(websocket)
