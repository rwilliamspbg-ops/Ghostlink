import { describe, expect, it } from "vitest";

import { iterSseJson } from "../src/sse.js";
import { StreamChunk } from "../src/models.js";

/**
 * Builds an SSE line stream the way the server would produce it, from a
 * list of already-JSON-encoded data payload strings. Mirrors
 * `sdks/python/tests/test_sse.py`'s `sse_lines` helper.
 */
function sseLines(...events: string[]): string[] {
  const lines: string[] = [];
  for (const event of events) {
    lines.push(`data: ${event}`);
    lines.push("");
  }
  return lines;
}

async function collect<T>(iter: AsyncIterable<T>): Promise<T[]> {
  const out: T[] = [];
  for await (const item of iter) out.push(item);
  return out;
}

describe("iterSseJson", () => {
  it("parses a single event", async () => {
    const lines = sseLines('{"token": "hello", "request_id": "req-1", "session_id": "s1"}');
    const events = await collect(iterSseJson(lines));
    expect(events).toEqual([{ token: "hello", request_id: "req-1", session_id: "s1" }]);
  });

  it("parses multiple events in order", async () => {
    const lines = sseLines(
      '{"token": "hel", "request_id": "req-1", "session_id": "s1"}',
      '{"token": "lo", "request_id": "req-1", "session_id": "s1"}',
      '{"done": true, "request_id": "req-1", "session_id": "s1", "truncated": false}',
    );
    const events = await collect(iterSseJson(lines));
    expect(events.slice(0, 2).map((e) => e.token)).toEqual(["hel", "lo"]);
    expect(events[2].done).toBe(true);
  });

  it("ignores comment lines", async () => {
    const lines = [
      ": ping",
      "",
      ...sseLines('{"token": "x", "request_id": "r", "session_id": "s"}'),
    ];
    const events = await collect(iterSseJson(lines));
    expect(events).toHaveLength(1);
    expect(events[0].token).toBe("x");
  });

  it("skips an invalid JSON payload", async () => {
    const lines = [
      "data: not json",
      "",
      ...sseLines('{"token": "ok", "request_id": "r", "session_id": "s"}'),
    ];
    const events = await collect(iterSseJson(lines));
    expect(events).toHaveLength(1);
    expect(events[0].token).toBe("ok");
  });

  it("handles a trailing event without a final blank line", async () => {
    const lines = ['data: {"token": "x", "request_id": "r", "session_id": "s"}'];
    const events = await collect(iterSseJson(lines));
    expect(events).toHaveLength(1);
  });
});

describe("StreamChunk.fromDict", () => {
  it("applies defaults for missing fields", () => {
    const chunk = StreamChunk.fromDict({ token: "hi", request_id: "r1", session_id: "s1" });
    expect(chunk.token).toBe("hi");
    expect(chunk.done).toBe(false);
    expect(chunk.error).toBe(false);
  });

  it("reads done/error/truncated flags", () => {
    const doneChunk = StreamChunk.fromDict({
      done: true,
      request_id: "r1",
      session_id: "s1",
      truncated: true,
    });
    expect(doneChunk.done).toBe(true);
    expect(doneChunk.truncated).toBe(true);

    const errorChunk = StreamChunk.fromDict({
      token: "[err]",
      error: true,
      request_id: "r1",
      session_id: "s1",
    });
    expect(errorChunk.error).toBe(true);
  });
});
