/**
 * Minimal Server-Sent Events parser for a `fetch` `Response.body` stream.
 *
 * Mirrors `sdks/python/ghostlink_client/_sse.py`. Ghostlink's streaming
 * endpoint (`/api/inference/chat` with `stream: true`) sends unnamed SSE
 * events — each a single `data: {json}` line followed by a blank line — so
 * this parser only needs to handle the `data:` field, but follows the SSE
 * spec generally (multiple `data:` lines per event are joined with `\n`,
 * comment lines starting with `:` are ignored) rather than assuming that
 * shape, so it keeps working if the server ever splits a payload across
 * lines.
 */

export type JsonObject = Record<string, unknown>;

/**
 * Decodes a raw byte stream (as `Response.body` from `fetch`) into a line
 * stream, the way `requests`' `iter_lines()` does for the Python client.
 * Handles chunk boundaries that split in the middle of a line, `\n` and
 * `\r\n` line endings, and a final line with no trailing newline.
 */
export async function* streamLines(body: ReadableStream<Uint8Array>): AsyncGenerator<string> {
  const reader = body.getReader();
  const decoder = new TextDecoder("utf-8");
  let buffer = "";
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      let newlineIndex: number;
      while ((newlineIndex = buffer.indexOf("\n")) !== -1) {
        let line = buffer.slice(0, newlineIndex);
        buffer = buffer.slice(newlineIndex + 1);
        if (line.endsWith("\r")) line = line.slice(0, -1);
        yield line;
      }
    }
    buffer += decoder.decode();
    if (buffer.length > 0) {
      yield buffer.endsWith("\r") ? buffer.slice(0, -1) : buffer;
    }
  } finally {
    reader.releaseLock();
  }
}

/**
 * Decodes an SSE line stream into one object per event's `data:` payload.
 *
 * A payload that isn't valid JSON is silently skipped rather than thrown —
 * a keep-alive comment or a non-JSON control line is valid SSE, not a
 * client-side bug.
 */
export async function* iterSseJson(
  lines: AsyncIterable<string> | Iterable<string>,
): AsyncGenerator<JsonObject> {
  const dataLines: string[] = [];

  function flush(): JsonObject | null {
    if (dataLines.length === 0) return null;
    const payload = dataLines.join("\n");
    dataLines.length = 0;
    let decoded: unknown;
    try {
      decoded = JSON.parse(payload);
    } catch {
      return null;
    }
    return decoded !== null && typeof decoded === "object" && !Array.isArray(decoded)
      ? (decoded as JsonObject)
      : null;
  }

  for await (const line of lines) {
    if (line === "") {
      const decoded = flush();
      if (decoded !== null) yield decoded;
      continue;
    }
    if (line.startsWith(":")) continue;
    if (line.startsWith("data:")) {
      dataLines.push(line.slice("data:".length).replace(/^ /, ""));
    }
    // event:/id:/retry: fields are ignored — the server only ever sends
    // unnamed data-only events today.
  }

  const decoded = flush();
  if (decoded !== null) yield decoded;
}
