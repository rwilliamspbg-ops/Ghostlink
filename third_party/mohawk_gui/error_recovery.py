"""
Error Recovery Manager for Mohawk Inference Engine GUI

Provides graceful degradation and automatic recovery mechanisms.
"""

import asyncio
from typing import Optional, Callable, Dict, Any
from dataclasses import dataclass, field
from enum import Enum


class RecoveryAction(Enum):
    """Recovery action types."""
    RETRY = "retry"
    DEGRADE = "degrade"
    ALERT = "alert"
    ABORT = "abort"
    IGNORE = "ignore"


@dataclass
class RecoveryStrategy:
    """Define how to handle specific error types."""
    error_type: str
    action: RecoveryAction
    parameters: Dict[str, Any] = field(default_factory=dict)


class ErrorRecoveryManager:
    """
    Handle errors gracefully with fallback strategies.
    
    Features:
    - Automatic retry with exponential backoff
    - Graceful degradation to fallback modes
    - Alert generation for critical failures
    - Transaction rollback support
    """
    
    def __init__(self, alert_callback: Callable = None):
        """
        Initialize error recovery manager.
        
        Args:
            alert_callback: Optional callback for alert notifications
        """
        self.strategies: Dict[str, RecoveryStrategy] = {
            "ConnectionTimeout": RecoveryStrategy(
                error_type="ConnectionTimeout",
                action=RecoveryAction.RETRY,
                parameters={
                    "max_retries": 5,
                    "initial_backoff_seconds": 1,
                    "max_backoff_seconds": 30
                }
            ),
            "WorkerOffline": RecoveryStrategy(
                error_type="WorkerOffline",
                action=RecoveryAction.DEGRADE,
                parameters={
                    "fallback_mode": "single_worker",
                    "alert_users": True
                }
            ),
            "SSLValidationError": RecoveryStrategy(
                error_type="SSLValidationError",
                action=RecoveryAction.ALERT,
                parameters={
                    "severity": "high",
                    "message": "SSL certificate validation failed"
                }
            ),
            "MemoryPressure": RecoveryStrategy(
                error_type="MemoryPressure",
                action=RecoveryAction.DEGRADE,
                parameters={
                    "threshold_mb": 80,
                    "action": "reduce_batch_size"
                }
            ),
            "ModelLoadingError": RecoveryStrategy(
                error_type="ModelLoadingError",
                action=RecoveryAction.ABORT,
                parameters={
                    "rollback_transaction": True
                }
            )
        }
        self.alert_callback = alert_callback
        self._recovery_count: Dict[str, int] = {}
    
    async def handle_error(self, error: Exception, context: Dict[str, Any]) -> Optional[Any]:
        """
        Handle error with appropriate recovery strategy.
        
        Args:
            error: Exception that occurred
            context: Context information for recovery decisions
            
        Returns:
            Result of recovery action or original result if no error
        """
        try:
            error_type = type(error).__name__
            strategy = self.strategies.get(error_type)
            
            if not strategy:
                # No specific strategy, use default (alert and ignore)
                await self._default_error_handler(error, context)
                return None
            
            result = await self._execute_recovery_strategy(strategy, error, context)
            return result
            
        except Exception as recovery_error:
            # Recovery itself failed, log and alert
            print(f"Recovery failed for {error_type}: {recovery_error}")
            if self.alert_callback:
                await self.alert_callback("Recovery failed", str(error), str(recovery_error))
            return None
    
    async def _execute_recovery_strategy(
        self, 
        strategy: RecoveryStrategy, 
        error: Exception, 
        context: Dict[str, Any]
    ) -> Optional[Any]:
        """Execute recovery strategy."""
        
        if strategy.action == RecoveryAction.RETRY:
            return await self._retry_operation(strategy, error, context)
        elif strategy.action == RecoveryAction.DEGRADE:
            return await self._degrade_operation(strategy, error, context)
        elif strategy.action == RecoveryAction.ALERT:
            await self._handle_alert(error, context, strategy.parameters)
            return None
        elif strategy.action == RecoveryAction.ABORT:
            return await self._abort_operation(strategy, error, context)
        else:
            return None
    
    async def _retry_operation(self, strategy: RecoveryStrategy, error: Exception, context: Dict[str, Any]):
        """Retry operation with exponential backoff."""
        params = strategy.parameters
        max_retries = params.get("max_retries", 5)
        initial_backoff = params.get("initial_backoff_seconds", 1)
        max_backoff = params.get("max_backoff_seconds", 30)
        
        for attempt in range(max_retries):
            try:
                # Wait with exponential backoff
                if attempt > 0:
                    wait_time = min(initial_backoff * (2 ** attempt), max_backoff)
                    await asyncio.sleep(wait_time)
                
                # Retry the operation
                result = await self._execute_with_context(context)
                return result
                
            except Exception as retry_error:
                if attempt == max_retries - 1:
                    # Last attempt failed, raise original error
                    raise error from retry_error
                else:
                    continue
        
        return None
    
    async def _degrade_operation(self, strategy: RecoveryStrategy, error: Exception, context: Dict[str, Any]):
        """Degraded operation with fallback."""
        params = strategy.parameters
        fallback_mode = params.get("fallback_mode", "single_worker")
        
        # Log degradation event
        print(f"Degraded to {fallback_mode} mode due to: {error}")
        
        # Execute in fallback mode
        return await self._execute_with_context(context, fallback_mode=fallback_mode)
    
    async def _abort_operation(self, strategy: RecoveryStrategy, error: Exception, context: Dict[str, Any]):
        """Abort operation and rollback if needed."""
        params = strategy.parameters
        
        if params.get("rollback_transaction", False):
            await self._rollback_transaction()
        
        print(f"Aborted operation due to: {error}")
        return None
    
    async def _handle_alert(self, error: Exception, context: Dict[str, Any], params: Dict[str, Any]):
        """Handle alert notification."""
        severity = params.get("severity", "medium")
        message = params.get("message", str(error))
        
        alert_data = {
            "error_type": type(error).__name__,
            "severity": severity,
            "message": message,
            "timestamp": __import__('datetime').datetime.now().isoformat()
        }
        
        if self.alert_callback:
            await self.alert_callback(severity, message, str(error))
    
    async def _default_error_handler(self, error: Exception, context: Dict[str, Any]):
        """Handle errors without specific strategy."""
        print(f"Unhandled error type: {type(error).__name__}: {error}")
        
        if self.alert_callback:
            await self.alert_callback("warning", f"Unhandled error: {type(error).__name__}", str(error))
    
    async def _rollback_transaction(self):
        """Rollback any in-flight transactions."""
        print("Rolling back transaction...")
        # Implementation depends on your transaction system
    
    async def _execute_with_context(
        self, 
        context: Dict[str, Any], 
        fallback_mode: str = None
    ) -> Any:
        """Execute operation with context."""
        # This would contain the actual operation logic
        # For example: await worker.process_request(request)
        return {"status": "success", "context": context}
    
    def get_recovery_stats(self) -> Dict[str, Any]:
        """Get recovery statistics."""
        total = sum(self._recovery_count.values())
        return {
            "total_recoveries": total,
            "by_type": self._recovery_count
        }


class DegradedModeManager:
    """Manage degraded operation modes."""
    
    def __init__(self):
        self.current_mode = "full"
        self.modes = {
            "full": {"batch_size": 32, "concurrency": 10},
            "single_worker": {"batch_size": 8, "concurrency": 2},
            "minimal": {"batch_size": 1, "concurrency": 1}
        }
    
    def enter_degraded_mode(self, mode: str):
        """Enter degraded operation mode."""
        if mode in self.modes:
            self.current_mode = mode
            print(f"Entered degraded mode: {mode}")
    
    def get_current_config(self) -> Dict[str, Any]:
        """Get current operational configuration."""
        return {
            "mode": self.current_mode,
            "config": self.modes.get(self.current_mode, {})
        }


if __name__ == "__main__":
    import asyncio
    
    async def test_recovery():
        recovery = ErrorRecoveryManager()
        
        # Simulate error handling
        try:
            raise ConnectionTimeout("Connection timed out")
        except Exception as e:
            result = await recovery.handle_error(e, {"operation": "infer"})
            print(f"Recovery result: {result}")
    
    class ConnectionTimeout(Exception):
        pass
    
    asyncio.run(test_recovery())
