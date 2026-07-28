"""Typed response wrappers.

These are thin, permissive views over the JSON the server returns — every
field is read with a default rather than a required key, so a minor server
version skew (a new field added, one renamed) doesn't raise inside the
client. The original decoded JSON is always available on `.raw` for
anything not modeled explicitly here.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional


@dataclass
class ChatChoice:
    index: int
    message: Dict[str, Any]
    finish_reason: Optional[str] = None

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "ChatChoice":
        return cls(
            index=data.get("index", 0),
            message=data.get("message", {}) or {},
            finish_reason=data.get("finish_reason"),
        )


@dataclass
class ChatCompletion:
    id: str
    object: str
    created: int
    model: str
    choices: List[ChatChoice]
    raw: Dict[str, Any] = field(repr=False, default_factory=dict)

    @property
    def content(self) -> str:
        """The first choice's assistant text, or `""` if there are none."""
        if not self.choices:
            return ""
        return self.choices[0].message.get("content", "")

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "ChatCompletion":
        return cls(
            id=data.get("id", ""),
            object=data.get("object", ""),
            created=data.get("created", 0),
            model=data.get("model", ""),
            choices=[ChatChoice.from_dict(c) for c in data.get("choices", [])],
            raw=data,
        )


@dataclass
class CompletionChoice:
    text: str
    index: int
    finish_reason: Optional[str] = None


@dataclass
class Completion:
    id: str
    object: str
    created: int
    model: str
    choices: List[CompletionChoice]
    raw: Dict[str, Any] = field(repr=False, default_factory=dict)

    @property
    def text(self) -> str:
        return self.choices[0].text if self.choices else ""

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Completion":
        return cls(
            id=data.get("id", ""),
            object=data.get("object", ""),
            created=data.get("created", 0),
            model=data.get("model", ""),
            choices=[
                CompletionChoice(
                    text=c.get("text", ""),
                    index=c.get("index", 0),
                    finish_reason=c.get("finish_reason"),
                )
                for c in data.get("choices", [])
            ],
            raw=data,
        )


@dataclass
class Model:
    id: str
    raw: Dict[str, Any] = field(repr=False, default_factory=dict)

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Model":
        return cls(id=data.get("id", ""), raw=data)


@dataclass
class StreamChunk:
    """One incremental piece of a `/api/inference/chat` streamed response."""

    token: str = ""
    request_id: str = ""
    session_id: str = ""
    error: bool = False
    done: bool = False
    truncated: bool = False
    raw: Dict[str, Any] = field(repr=False, default_factory=dict)

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "StreamChunk":
        return cls(
            token=data.get("token", ""),
            request_id=data.get("request_id", ""),
            session_id=data.get("session_id", ""),
            error=bool(data.get("error", False)),
            done=bool(data.get("done", False)),
            truncated=bool(data.get("truncated", False)),
            raw=data,
        )
