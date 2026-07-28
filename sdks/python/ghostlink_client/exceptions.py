"""Exception hierarchy for the Ghostlink client."""

from __future__ import annotations

from typing import Any


class GhostlinkError(Exception):
    """Base class for all errors raised by this client."""


class GhostlinkConnectionError(GhostlinkError):
    """The client could not reach the Ghostlink server at all — DNS
    failure, connection refused, or a timeout before any response arrived.
    """


class GhostlinkAPIError(GhostlinkError):
    """The server responded with a non-2xx status code."""

    def __init__(self, status_code: int, message: str, body: Any = None) -> None:
        super().__init__(f"HTTP {status_code}: {message}")
        self.status_code = status_code
        self.message = message
        self.body = body


class GhostlinkAuthError(GhostlinkAPIError):
    """401/403 — missing, invalid, or expired bearer token."""
