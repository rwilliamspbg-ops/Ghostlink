"""Client for a Ghostlink Studio API server.

Wraps both the OpenAI-compatible REST surface (`/v1/chat/completions`,
`/v1/completions`, `/v1/embeddings`, `/v1/models`) and Ghostlink's native
`/api/*` surface (workers, sessions, settings, metrics, and real
token-by-token streaming chat via `/api/inference/chat`).
"""

from __future__ import annotations

from typing import Any, Dict, Iterator, List, Mapping, Optional, Sequence, Union

import requests

from ._sse import iter_sse_json
from .exceptions import (
    GhostlinkAPIError,
    GhostlinkAuthError,
    GhostlinkConnectionError,
)
from .models import ChatCompletion, Completion, Model, StreamChunk

__all__ = ["GhostlinkClient"]

DEFAULT_TIMEOUT = 30.0


def _add_optional(payload: Dict[str, Any], **fields: Any) -> None:
    for key, value in fields.items():
        if value is not None:
            payload[key] = value


def _extract_error_message(body: Any) -> Optional[str]:
    if isinstance(body, dict):
        error = body.get("error")
        if isinstance(error, dict):
            return error.get("message")
        if isinstance(error, str):
            return error
    return None


class _ChatCompletions:
    def __init__(self, client: "GhostlinkClient") -> None:
        self._client = client

    def create(
        self,
        *,
        model: str,
        messages: Sequence[Mapping[str, Any]],
        temperature: Optional[float] = None,
        top_p: Optional[float] = None,
        top_k: Optional[int] = None,
        penalty: Optional[float] = None,
        max_tokens: Optional[int] = None,
    ) -> ChatCompletion:
        """`POST /v1/chat/completions` — OpenAI-compatible, non-streaming."""
        payload: Dict[str, Any] = {"model": model, "messages": list(messages)}
        _add_optional(
            payload,
            temperature=temperature,
            top_p=top_p,
            top_k=top_k,
            penalty=penalty,
            max_tokens=max_tokens,
        )
        data = self._client._request("POST", "/v1/chat/completions", json=payload)
        return ChatCompletion.from_dict(data)


class _Chat:
    def __init__(self, client: "GhostlinkClient") -> None:
        self.completions = _ChatCompletions(client)


class _Completions:
    def __init__(self, client: "GhostlinkClient") -> None:
        self._client = client

    def create(
        self,
        *,
        model: str,
        prompt: str,
        temperature: Optional[float] = None,
        top_p: Optional[float] = None,
        top_k: Optional[int] = None,
        penalty: Optional[float] = None,
        max_tokens: Optional[int] = None,
    ) -> Completion:
        """`POST /v1/completions` — OpenAI's legacy completions endpoint."""
        payload: Dict[str, Any] = {"model": model, "prompt": prompt}
        _add_optional(
            payload,
            temperature=temperature,
            top_p=top_p,
            top_k=top_k,
            penalty=penalty,
            max_tokens=max_tokens,
        )
        data = self._client._request("POST", "/v1/completions", json=payload)
        return Completion.from_dict(data)


class _Embeddings:
    def __init__(self, client: "GhostlinkClient") -> None:
        self._client = client

    def create(self, *, model: str, input: Union[str, Sequence[str]]) -> Dict[str, Any]:
        """`POST /v1/embeddings`."""
        return self._client._request(
            "POST", "/v1/embeddings", json={"model": model, "input": input}
        )


class _Models:
    def __init__(self, client: "GhostlinkClient") -> None:
        self._client = client

    def list(self) -> List[Model]:
        """`GET /v1/models`."""
        data = self._client._request("GET", "/v1/models")
        return [Model.from_dict(m) for m in data.get("data", [])]


class _Workers:
    def __init__(self, client: "GhostlinkClient") -> None:
        self._client = client

    def list(self) -> List[Dict[str, Any]]:
        return self._client._request("GET", "/api/workers").get("workers", [])

    def discover(self) -> Dict[str, Any]:
        """Triggers a UDP-broadcast + mDNS discovery sweep and registers
        any peers found. See `GET /api/workers/discover`."""
        return self._client._request("GET", "/api/workers/discover")

    def connect(self, host: str, port: int) -> Dict[str, Any]:
        return self._client._request(
            "POST", "/api/workers/connect", json={"host": host, "port": port}
        )

    def add(self, **fields: Any) -> Dict[str, Any]:
        return self._client._request("POST", "/api/workers/add", json=fields)

    def disconnect(self, worker_id: str) -> Dict[str, Any]:
        return self._client._request("POST", f"/api/workers/{worker_id}/disconnect")


class _Sessions:
    def __init__(self, client: "GhostlinkClient") -> None:
        self._client = client

    def list(self) -> List[Dict[str, Any]]:
        return self._client._request("GET", "/api/sessions").get("sessions", [])


class _Settings:
    def __init__(self, client: "GhostlinkClient") -> None:
        self._client = client

    def get(self) -> Dict[str, Any]:
        return self._client._request("GET", "/api/settings")

    def update(self, **fields: Any) -> Dict[str, Any]:
        return self._client._request("POST", "/api/settings", json=fields)

    def reset(self) -> Dict[str, Any]:
        return self._client._request("POST", "/api/settings/reset")


class GhostlinkClient:
    """Client for a Ghostlink Studio API server.

    Example:
        >>> client = GhostlinkClient("http://127.0.0.1:8003", api_key="...")
        >>> resp = client.chat.completions.create(
        ...     model="llama3.2:3b",
        ...     messages=[{"role": "user", "content": "hello"}],
        ... )
        >>> print(resp.content)

    Use as a context manager to close the underlying HTTP session:
        >>> with GhostlinkClient(url, api_key=key) as client:
        ...     client.health()
    """

    def __init__(
        self,
        base_url: str,
        api_key: Optional[str] = None,
        timeout: float = DEFAULT_TIMEOUT,
        session: Optional[requests.Session] = None,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.api_key = api_key
        self.timeout = timeout
        self._session = session or requests.Session()

        self.chat = _Chat(self)
        self.completions = _Completions(self)
        self.embeddings = _Embeddings(self)
        self.models = _Models(self)
        self.workers = _Workers(self)
        self.sessions = _Sessions(self)
        self.settings = _Settings(self)

    def __enter__(self) -> "GhostlinkClient":
        return self

    def __exit__(self, *exc_info: object) -> None:
        self.close()

    def close(self) -> None:
        self._session.close()

    def _headers(self) -> Dict[str, str]:
        headers = {"Content-Type": "application/json"}
        if self.api_key:
            headers["Authorization"] = f"Bearer {self.api_key}"
        return headers

    def _raise_for_status(self, resp: requests.Response) -> None:
        if resp.status_code < 400:
            return
        body: Any
        try:
            body = resp.json()
        except ValueError:
            body = resp.text
        message = _extract_error_message(body) or resp.reason or "request failed"
        if resp.status_code in (401, 403):
            raise GhostlinkAuthError(resp.status_code, message, body)
        raise GhostlinkAPIError(resp.status_code, message, body)

    def _request(self, method: str, path: str, **kwargs: Any) -> Dict[str, Any]:
        url = f"{self.base_url}{path}"
        try:
            resp = self._session.request(
                method, url, headers=self._headers(), timeout=self.timeout, **kwargs
            )
        except requests.exceptions.RequestException as err:
            raise GhostlinkConnectionError(f"failed to reach {url}: {err}") from err

        self._raise_for_status(resp)

        if not resp.content:
            return {}
        return resp.json()

    def health(self) -> Dict[str, Any]:
        """`GET /health` — unauthenticated liveness/version/backend info."""
        return self._request("GET", "/health")

    def metrics(self) -> Dict[str, Any]:
        """`GET /api/metrics` — JSON metrics snapshot. See
        `metrics_prometheus` for the Prometheus-exposition-format sibling."""
        return self._request("GET", "/api/metrics")

    def metrics_prometheus(self) -> str:
        """`GET /metrics` — the same snapshot as `metrics()`, formatted for
        a Prometheus scrape config instead of JSON."""
        url = f"{self.base_url}/metrics"
        try:
            resp = self._session.get(url, headers=self._headers(), timeout=self.timeout)
        except requests.exceptions.RequestException as err:
            raise GhostlinkConnectionError(f"failed to reach {url}: {err}") from err
        self._raise_for_status(resp)
        return resp.text

    def list_models(self) -> List[Model]:
        """Convenience alias for ``self.models.list()``."""
        return self.models.list()

    def studio_chat(
        self,
        message: str,
        *,
        messages: Optional[Sequence[Mapping[str, str]]] = None,
        model: Optional[str] = None,
        temperature: Optional[float] = None,
        top_p: Optional[float] = None,
        top_k: Optional[int] = None,
        penalty: Optional[float] = None,
        max_tokens: Optional[int] = None,
    ) -> Dict[str, Any]:
        """Non-streaming call to Ghostlink Studio's native chat endpoint
        (`POST /api/inference/chat`) — richer than the OpenAI-compatible
        routes (session bookkeeping, MCP tool results, live metrics in the
        response), but Ghostlink-specific rather than portable. Use
        `stream_chat` for real token-by-token streaming.
        """
        payload: Dict[str, Any] = {"message": message}
        if messages is not None:
            payload["messages"] = list(messages)
        _add_optional(
            payload,
            model=model,
            temperature=temperature,
            top_p=top_p,
            top_k=top_k,
            penalty=penalty,
            max_tokens=max_tokens,
        )
        return self._request("POST", "/api/inference/chat", json=payload)

    def stream_chat(
        self,
        message: str,
        *,
        messages: Optional[Sequence[Mapping[str, str]]] = None,
        model: Optional[str] = None,
        temperature: Optional[float] = None,
        top_p: Optional[float] = None,
        top_k: Optional[int] = None,
        penalty: Optional[float] = None,
        max_tokens: Optional[int] = None,
    ) -> Iterator[StreamChunk]:
        """Real token-by-token streaming via `POST /api/inference/chat`
        (`stream: true`) over Server-Sent Events. Yields one `StreamChunk`
        per token as the model produces it; the final chunk has
        `done=True`. This is the only endpoint with genuine incremental
        streaming today — the OpenAI-compatible `/v1/chat/completions`
        accepts a `stream` field but does not yet act on it.

        Example:
            >>> for chunk in client.stream_chat("tell me a story"):
            ...     if not chunk.done:
            ...         print(chunk.token, end="", flush=True)
        """
        payload: Dict[str, Any] = {"message": message, "stream": True}
        if messages is not None:
            payload["messages"] = list(messages)
        _add_optional(
            payload,
            model=model,
            temperature=temperature,
            top_p=top_p,
            top_k=top_k,
            penalty=penalty,
            max_tokens=max_tokens,
        )
        url = f"{self.base_url}/api/inference/chat"
        try:
            resp = self._session.post(
                url,
                headers=self._headers(),
                json=payload,
                timeout=self.timeout,
                stream=True,
            )
        except requests.exceptions.RequestException as err:
            raise GhostlinkConnectionError(f"failed to reach {url}: {err}") from err

        self._raise_for_status(resp)

        for payload_dict in iter_sse_json(resp.iter_lines(decode_unicode=True)):
            yield StreamChunk.from_dict(payload_dict)
