import { describe, expect, it, vi } from "vitest";

import {
  GhostlinkAPIError,
  GhostlinkAuthError,
  GhostlinkClient,
  GhostlinkConnectionError,
} from "../src/index.js";

const BASE_URL = "http://127.0.0.1:8003";

/**
 * Builds a mock `fetch` that mirrors the Python test suite's use of the
 * `responses` library: register one JSON (or text) response per URL+method,
 * and assert on the captured request afterwards.
 */
function mockFetch(responseFor: (url: string, init: RequestInit) => Response) {
  const calls: { url: string; init: RequestInit }[] = [];
  const fn = vi.fn(async (input: string | URL | Request, init: RequestInit = {}) => {
    const url = typeof input === "string" ? input : input.toString();
    calls.push({ url, init });
    return responseFor(url, init);
  });
  return { fetchImpl: fn as unknown as typeof fetch, calls };
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function client(fetchImpl: typeof fetch): GhostlinkClient {
  return new GhostlinkClient(BASE_URL, { apiKey: "test-key", fetchImpl });
}

describe("GhostlinkClient", () => {
  it("chat.completions.create returns a ChatCompletion and sends the bearer token", async () => {
    const { fetchImpl, calls } = mockFetch(() =>
      jsonResponse({
        id: "chatcmpl-1",
        object: "chat.completion",
        created: 123,
        model: "llama3.2:3b",
        choices: [
          {
            index: 0,
            message: { role: "assistant", content: "hi there" },
            finish_reason: "stop",
          },
        ],
      }),
    );

    const resp = await client(fetchImpl).chat.completions.create({
      model: "llama3.2:3b",
      messages: [{ role: "user", content: "hello" }],
    });

    expect(resp.content).toBe("hi there");
    expect(resp.model).toBe("llama3.2:3b");
    const headers = calls[0].init.headers as Record<string, string>;
    expect(headers["Authorization"]).toBe("Bearer test-key");
    expect(calls[0].url).toBe(`${BASE_URL}/v1/chat/completions`);
  });

  it("completions.create returns a Completion", async () => {
    const { fetchImpl } = mockFetch(() =>
      jsonResponse({
        id: "cmpl-1",
        object: "text_completion",
        created: 123,
        model: "llama3.2:3b",
        choices: [{ text: "once upon a time", index: 0, finish_reason: "stop" }],
      }),
    );

    const resp = await client(fetchImpl).completions.create({
      model: "llama3.2:3b",
      prompt: "Once",
    });

    expect(resp.text).toBe("once upon a time");
  });

  it("listModels returns Model instances", async () => {
    const { fetchImpl } = mockFetch(() =>
      jsonResponse({ data: [{ id: "llama3.2:3b" }, { id: "gemma2:2b" }] }),
    );

    const models = await client(fetchImpl).listModels();
    expect(models.map((m) => m.id)).toEqual(["llama3.2:3b", "gemma2:2b"]);
  });

  it("raises GhostlinkAuthError on a 401", async () => {
    const { fetchImpl } = mockFetch(() =>
      jsonResponse(
        { error: { message: "missing bearer token", type: "invalid_request_error" } },
        401,
      ),
    );

    let caught: unknown;
    try {
      await client(fetchImpl).chat.completions.create({
        model: "x",
        messages: [{ role: "user", content: "hi" }],
      });
    } catch (err) {
      caught = err;
    }
    expect(caught).toBeInstanceOf(GhostlinkAuthError);
    const authErr = caught as GhostlinkAuthError;
    expect(authErr.statusCode).toBe(401);
    expect(authErr.message).toContain("missing bearer token");
  });

  it("raises GhostlinkAPIError on a 400", async () => {
    const { fetchImpl } = mockFetch(() =>
      jsonResponse(
        { error: { message: "messages must not be empty", type: "invalid_request_error" } },
        400,
      ),
    );

    let caught: unknown;
    try {
      await client(fetchImpl).chat.completions.create({ model: "x", messages: [] });
    } catch (err) {
      caught = err;
    }
    expect(caught).toBeInstanceOf(GhostlinkAPIError);
    expect(caught).not.toBeInstanceOf(GhostlinkAuthError);
    const apiErr = caught as GhostlinkAPIError;
    expect(apiErr.statusCode).toBe(400);
    expect(apiErr.message).toContain("messages must not be empty");
  });

  it("raises GhostlinkConnectionError when fetch itself fails", async () => {
    const fetchImpl = vi.fn(async () => {
      throw new TypeError("fetch failed");
    }) as unknown as typeof fetch;

    const unreachable = new GhostlinkClient(BASE_URL, { apiKey: "k", fetchImpl });
    await expect(unreachable.health()).rejects.toBeInstanceOf(GhostlinkConnectionError);
  });

  it("health returns the raw JSON body", async () => {
    const { fetchImpl } = mockFetch(() => jsonResponse({ status: "healthy" }));
    expect(await client(fetchImpl).health()).toEqual({ status: "healthy" });
  });

  it("metricsPrometheus returns raw text, not parsed JSON", async () => {
    const { fetchImpl } = mockFetch(
      () =>
        new Response("# HELP ghostlink_uptime_seconds ...\nghostlink_uptime_seconds 12.5\n", {
          status: 200,
          headers: { "Content-Type": "text/plain; version=0.0.4; charset=utf-8" },
        }),
    );
    const text = await client(fetchImpl).metricsPrometheus();
    expect(text).toContain("ghostlink_uptime_seconds 12.5");
  });

  it("settings.update posts the given fields and returns the response", async () => {
    const { fetchImpl, calls } = mockFetch(() =>
      jsonResponse({ settings: { inference_backend: "ollama" }, status: "ok" }),
    );

    const result = await client(fetchImpl).settings.update({ inference_backend: "ollama" });
    expect(result.status).toBe("ok");
    expect(calls[0].init.body).toContain("ollama");
  });

  it("streamChat yields one StreamChunk per token via /api/inference/chat", async () => {
    const events = [
      { token: "Once", request_id: "req-1", session_id: "s1" },
      { token: " upon", request_id: "req-1", session_id: "s1" },
      { token: "", request_id: "req-1", session_id: "s1", done: true, truncated: false },
    ];
    const sseBody = events.map((e) => `data: ${JSON.stringify(e)}\n\n`).join("");

    const fetchImpl = vi.fn(async (input: string | URL | Request, init: RequestInit = {}) => {
      const url = typeof input === "string" ? input : input.toString();
      expect(url).toBe(`${BASE_URL}/api/inference/chat`);
      expect(JSON.parse(init.body as string)).toMatchObject({
        message: "tell me a story",
        stream: true,
      });
      const bytes = new TextEncoder().encode(sseBody);
      const stream = new ReadableStream<Uint8Array>({
        start(controller) {
          controller.enqueue(bytes);
          controller.close();
        },
      });
      return new Response(stream, {
        status: 200,
        headers: { "Content-Type": "text/event-stream" },
      });
    }) as unknown as typeof fetch;

    const chunks = [];
    for await (const chunk of client(fetchImpl).streamChat("tell me a story")) {
      chunks.push(chunk);
    }

    expect(chunks.map((c) => c.token)).toEqual(["Once", " upon", ""]);
    expect(chunks[chunks.length - 1].done).toBe(true);
    expect(chunks.every((c) => !c.error)).toBe(true);
  });
});
