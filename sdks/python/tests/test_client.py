import pytest
import responses

from ghostlink_client import (
    GhostlinkAPIError,
    GhostlinkAuthError,
    GhostlinkClient,
    GhostlinkConnectionError,
)

BASE_URL = "http://127.0.0.1:8003"


@pytest.fixture
def client():
    with GhostlinkClient(BASE_URL, api_key="test-key") as c:
        yield c


@responses.activate
def test_chat_completions_create(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/v1/chat/completions",
        json={
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "created": 123,
            "model": "llama3.2:3b",
            "choices": [
                {
                    "index": 0,
                    "message": {"role": "assistant", "content": "hi there"},
                    "finish_reason": "stop",
                }
            ],
        },
        status=200,
    )

    resp = client.chat.completions.create(
        model="llama3.2:3b", messages=[{"role": "user", "content": "hello"}]
    )

    assert resp.content == "hi there"
    assert resp.model == "llama3.2:3b"
    assert responses.calls[0].request.headers["Authorization"] == "Bearer test-key"


@responses.activate
def test_completions_create(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/v1/completions",
        json={
            "id": "cmpl-1",
            "object": "text_completion",
            "created": 123,
            "model": "llama3.2:3b",
            "choices": [{"text": "once upon a time", "index": 0, "finish_reason": "stop"}],
        },
        status=200,
    )

    resp = client.completions.create(model="llama3.2:3b", prompt="Once")
    assert resp.text == "once upon a time"


@responses.activate
def test_list_models(client):
    responses.add(
        responses.GET,
        f"{BASE_URL}/v1/models",
        json={"data": [{"id": "llama3.2:3b"}, {"id": "gemma2:2b"}]},
        status=200,
    )

    models = client.list_models()
    assert [m.id for m in models] == ["llama3.2:3b", "gemma2:2b"]


@responses.activate
def test_auth_error_raises_ghostlink_auth_error(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/v1/chat/completions",
        json={"error": {"message": "missing bearer token", "type": "invalid_request_error"}},
        status=401,
    )

    with pytest.raises(GhostlinkAuthError) as exc_info:
        client.chat.completions.create(model="x", messages=[{"role": "user", "content": "hi"}])

    assert exc_info.value.status_code == 401
    assert "missing bearer token" in str(exc_info.value)


@responses.activate
def test_bad_request_raises_ghostlink_api_error(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/v1/chat/completions",
        json={"error": {"message": "messages must not be empty", "type": "invalid_request_error"}},
        status=400,
    )

    with pytest.raises(GhostlinkAPIError) as exc_info:
        client.chat.completions.create(model="x", messages=[])

    assert exc_info.value.status_code == 400
    assert "messages must not be empty" in exc_info.value.message


def test_connection_error_raises_ghostlink_connection_error(client):
    # No `responses.activate` — nothing is mocked, so the request against an
    # address nothing is listening on raises requests' own connection error,
    # which the client should wrap.
    unreachable = GhostlinkClient("http://127.0.0.1:1", api_key="k", timeout=1)
    with pytest.raises(GhostlinkConnectionError):
        unreachable.health()


@responses.activate
def test_health(client):
    responses.add(responses.GET, f"{BASE_URL}/health", json={"status": "healthy"}, status=200)
    assert client.health() == {"status": "healthy"}


@responses.activate
def test_metrics_prometheus_returns_raw_text(client):
    responses.add(
        responses.GET,
        f"{BASE_URL}/metrics",
        body="# HELP ghostlink_uptime_seconds ...\nghostlink_uptime_seconds 12.5\n",
        status=200,
        content_type="text/plain; version=0.0.4; charset=utf-8",
    )
    text = client.metrics_prometheus()
    assert "ghostlink_uptime_seconds 12.5" in text


@responses.activate
def test_settings_update(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/api/settings",
        json={"settings": {"inference_backend": "ollama"}, "status": "ok"},
        status=200,
    )
    result = client.settings.update(inference_backend="ollama")
    assert result["status"] == "ok"
    sent_body = responses.calls[0].request.body
    assert b"ollama" in sent_body
