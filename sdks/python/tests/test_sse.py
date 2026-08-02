from ghostlink_client._sse import iter_sse_json
from ghostlink_client.models import StreamChunk


def sse_lines(*events):
    """Builds an SSE line stream (as `requests.Response.iter_lines()` would
    yield it) from a list of already-JSON-encoded data payload strings.
    """
    lines = []
    for event in events:
        lines.append(f"data: {event}")
        lines.append("")
    return lines


def test_parses_single_event():
    lines = sse_lines('{"token": "hello", "request_id": "req-1", "session_id": "s1"}')
    events = list(iter_sse_json(lines))
    assert events == [{"token": "hello", "request_id": "req-1", "session_id": "s1"}]


def test_parses_multiple_events_in_order():
    lines = sse_lines(
        '{"token": "hel", "request_id": "req-1", "session_id": "s1"}',
        '{"token": "lo", "request_id": "req-1", "session_id": "s1"}',
        '{"done": true, "request_id": "req-1", "session_id": "s1", "truncated": false}',
    )
    events = list(iter_sse_json(lines))
    assert [e.get("token") for e in events[:2]] == ["hel", "lo"]
    assert events[2]["done"] is True


def test_ignores_comment_lines():
    lines = [": ping", ""] + sse_lines('{"token": "x", "request_id": "r", "session_id": "s"}')
    events = list(iter_sse_json(lines))
    assert len(events) == 1
    assert events[0]["token"] == "x"


def test_skips_invalid_json_payload():
    lines = ["data: not json", ""] + sse_lines('{"token": "ok", "request_id": "r", "session_id": "s"}')
    events = list(iter_sse_json(lines))
    assert len(events) == 1
    assert events[0]["token"] == "ok"


def test_handles_trailing_event_without_final_blank_line():
    lines = ['data: {"token": "x", "request_id": "r", "session_id": "s"}']
    events = list(iter_sse_json(lines))
    assert len(events) == 1


def test_stream_chunk_from_dict_defaults():
    chunk = StreamChunk.from_dict({"token": "hi", "request_id": "r1", "session_id": "s1"})
    assert chunk.token == "hi"
    assert chunk.done is False
    assert chunk.error is False


def test_stream_chunk_from_dict_done_and_error_flags():
    done_chunk = StreamChunk.from_dict({"done": True, "request_id": "r1", "session_id": "s1", "truncated": True})
    assert done_chunk.done is True
    assert done_chunk.truncated is True

    error_chunk = StreamChunk.from_dict({"token": "[err]", "error": True, "request_id": "r1", "session_id": "s1"})
    assert error_chunk.error is True
