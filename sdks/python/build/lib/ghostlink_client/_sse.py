"""Minimal Server-Sent Events parser for `requests`' `iter_lines()` output.

Ghostlink's streaming endpoint (`/api/inference/chat` with `stream: true`)
sends unnamed SSE events — each a single `data: {json}` line followed by a
blank line — so this parser only needs to handle the `data:` field, but
follows the SSE spec generally (multiple `data:` lines per event are
joined with `\n`, comment lines starting with `:` are ignored) rather than
assuming that shape, so it keeps working if the server ever splits a
payload across lines.
"""

from __future__ import annotations

import json
from typing import Any, Dict, Iterable, Iterator, Optional, Union


def iter_sse_json(lines: Iterable[Union[bytes, str]]) -> Iterator[Dict[str, Any]]:
    """Decodes an SSE line stream into one dict per event's `data:` payload.

    A payload that isn't valid JSON is silently skipped rather than raised —
    a keep-alive comment or a non-JSON control line is valid SSE, not a
    client-side bug.
    """
    data_lines = []

    def flush() -> Optional[Dict[str, Any]]:
        if not data_lines:
            return None
        payload = "\n".join(data_lines)
        data_lines.clear()
        try:
            decoded = json.loads(payload)
        except json.JSONDecodeError:
            return None
        return decoded if isinstance(decoded, dict) else None

    for raw_line in lines:
        line = raw_line.decode("utf-8") if isinstance(raw_line, bytes) else raw_line
        if line == "":
            decoded = flush()
            if decoded is not None:
                yield decoded
            continue
        if line.startswith(":"):
            continue
        if line.startswith("data:"):
            data_lines.append(line[len("data:") :].lstrip(" "))
        # event:/id:/retry: fields are ignored — the server only ever sends
        # unnamed data-only events today.

    decoded = flush()
    if decoded is not None:
        yield decoded
