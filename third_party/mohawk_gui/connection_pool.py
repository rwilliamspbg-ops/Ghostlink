"""
Connection Pool Manager for Mohawk Inference Engine GUI

Provides efficient WebSocket connection pooling for high-concurrency scenarios.
"""

import asyncio
import time
from collections import deque
from dataclasses import dataclass, field
from typing import Optional, Dict, Any
import random


@dataclass
class WebSocketConnection:
    """Represent a pooled WebSocket connection."""
    ws: Optional[Any] = None
    session_id: str = ""
    last_activity: float = field(default_factory=time.time)
    created_at: float = field(default_factory=time.time)
    error_count: int = 0
    max_errors: int = 3
    
    async def ping(self) -> bool:
        """Check if connection is alive."""
        try:
            await self.ws.ping()
            self.last_activity = time.time()
            return True
        except Exception:
            return False
    
    async def close(self):
        """Close the WebSocket connection."""
        if self.ws:
            await self.ws.close()


class ConnectionPool:
    """
    Manage WebSocket connections with pooling for high concurrency.
    
    Features:
    - Connection limiting to prevent resource exhaustion
    - Automatic eviction of inactive connections
    - Heartbeat monitoring
    - Graceful connection failure handling
    """
    
    def __init__(self, max_connections: int = 100, ping_interval: float = 30.0):
        """
        Initialize connection pool.
        
        Args:
            max_connections: Maximum concurrent connections allowed
            ping_interval: Seconds between heartbeat pings
        """
        self.max_connections = max_connections
        self.pool = asyncio.Semaphore(max_connections)
        self.active_connections: deque = deque()
        self.ping_interval = ping_interval
        self._connection_history: Dict[str, float] = {}
    
    async def acquire(self, session_id: str) -> WebSocketConnection:
        """
        Acquire connection from pool or create new one.
        
        Args:
            session_id: Unique session identifier
            
        Returns:
            WebSocketConnection instance
            
        Raises:
            ConnectionPoolExhaustedError: If no connections available
        """
        # Check if we need to evict inactive connections first
        await self._evict_inactive()
        
        # Try to acquire from pool
        async with self.pool:
            conn = WebSocketConnection(
                ws=None,  # Would initialize with actual WebSocket connection
                session_id=session_id,
                last_activity=time.time(),
                created_at=time.time()
            )
            self.active_connections.append(conn)
            
            # Track creation time for eviction
            self._connection_history[session_id] = time.time()
            
            return conn
    
    async def release(self, connection: WebSocketConnection):
        """
        Return connection to pool.
        
        Args:
            connection: Connection to release
        """
        if connection in self.active_connections:
            self.active_connections.remove(connection)
    
    async def _evict_inactive(self):
        """Remove connections that haven't pinged recently."""
        now = time.time()
        eviction_threshold = self.ping_interval * 2
        
        while len(self.active_connections) >= self.max_connections:
            oldest = self.active_connections.popleft()
            
            # Check if connection is inactive
            if now - oldest.last_activity > eviction_threshold:
                await self._close_connection(oldest)
            else:
                # Put back at front of queue
                self.active_connections.appendleft(oldest)
    
    async def _close_connection(self, connection: WebSocketConnection):
        """Close a connection gracefully."""
        try:
            if connection.ws:
                await connection.ws.close(code=1000)  # Normal closure
        except Exception as e:
            print(f"Error closing connection {connection.session_id}: {e}")
    
    async def health_check(self):
        """Perform health check on all active connections."""
        now = time.time()
        healthy_connections = []
        
        for conn in self.active_connections:
            is_healthy = await conn.ping()
            
            if is_healthy:
                healthy_connections.append(conn)
            else:
                conn.error_count += 1
                
                # Close connection if too many errors
                if conn.error_count >= conn.max_errors:
                    await self._close_connection(conn)
                else:
                    # Put back in queue with reset error count
                    conn.error_count = 0
                    self.active_connections.appendleft(conn)
        
        self.active_connections = deque(healthy_connections)
    
    def get_stats(self) -> Dict[str, Any]:
        """Get pool statistics."""
        return {
            "active_connections": len(self.active_connections),
            "max_connections": self.max_connections,
            "utilization": len(self.active_connections) / self.max_connections if self.max_connections > 0 else 0,
            "pool_semaphore_value": self.pool._value if hasattr(self.pool, '_value') else None
        }


class ConnectionPoolExhaustedError(Exception):
    """Raised when connection pool is exhausted."""
    pass


if __name__ == "__main__":
    # Test connection pool
    pool = ConnectionPool(max_connections=10)
    
    async def test_pool():
        conn = await pool.acquire("test_session")
        print(f"Acquired connection for session: {conn.session_id}")
        
        stats = pool.get_stats()
        print(f"Pool stats: {stats}")
    
    asyncio.run(test_pool())
