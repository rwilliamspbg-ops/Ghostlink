# ghostlink-client

JavaScript/TypeScript client for a [Ghostlink](https://github.com/rwilliamspbg-ops/Ghostlink) Studio API server — the OpenAI-compatible REST endpoints (`/v1/chat/completions`, `/v1/completions`, `/v1/embeddings`, `/v1/models`) plus Ghostlink's native `/api/*` surface (workers, sessions, settings, metrics, and real token-by-token streaming chat).

This is the JS/TS counterpart of `sdks/python` — same method names, same nested structure (`client.chat.completions.create`, `client.workers.connect`, ...), same endpoints. Built on the native `fetch`/`ReadableStream` APIs, so it has no runtime dependencies and works in Node 18+ and modern browsers alike.

## Install

```bash
npm install ghostlink-client
```

(Not yet published to npm — install from a local checkout or a Git URL until then.)

## Quick start

```ts
import { GhostlinkClient } from "ghostlink-client";

const client = new GhostlinkClient("http://127.0.0.1:8003", { apiKey: "<your api key>" });

const resp = await client.chat.completions.create({
  model: "llama3.2:3b",
  messages: [{ role: "user", content: "Say hello in one sentence." }],
});
console.log(resp.content);
```

The API key is the one Ghostlink generates on first run (printed at startup, persisted to `api_key.txt` by default — see `GHOSTLINK_API_KEY_PATH`).

## OpenAI-compatible endpoints

```ts
// Chat
const resp = await client.chat.completions.create({ model: "llama3.2:3b", messages: [...] });

// Legacy text completion
const cmpl = await client.completions.create({ model: "llama3.2:3b", prompt: "Once upon a time" });
console.log(cmpl.text);

// Embeddings
const emb = await client.embeddings.create({ model: "nomic-embed-text", input: "hello world" });

// List models
for (const m of await client.listModels()) {
  console.log(m.id);
}
```

## Real streaming chat

`/v1/chat/completions` accepts a `stream` field but Ghostlink doesn't act on it yet — real token-by-token streaming is only available today through Ghostlink Studio's native chat endpoint:

```ts
for await (const chunk of client.streamChat("Tell me a short story")) {
  if (chunk.error) {
    console.log(`\n[error: ${chunk.token}]`);
  } else if (!chunk.done) {
    process.stdout.write(chunk.token);
  }
}
```

## Cluster, metrics, settings

```ts
await client.workers.list();
await client.workers.discover();       // UDP broadcast + mDNS sweep
await client.workers.connect("192.168.1.42", 8003);

await client.metrics();                // JSON snapshot
await client.metricsPrometheus();      // Prometheus exposition format

await client.settings.get();
await client.settings.update({ inference_backend: "ollama" });
```

## Error handling

```ts
import { GhostlinkAPIError, GhostlinkAuthError, GhostlinkConnectionError } from "ghostlink-client";

try {
  await client.chat.completions.create({ model: "x", messages: [] });
} catch (err) {
  if (err instanceof GhostlinkAuthError) {
    console.log("bad or missing API key:", err.message);
  } else if (err instanceof GhostlinkAPIError) {
    console.log(`server rejected the request: HTTP ${err.statusCode}: ${err.message}`);
  } else if (err instanceof GhostlinkConnectionError) {
    console.log("could not reach the server:", err.message);
  } else {
    throw err;
  }
}
```

## Custom inference backends

If the server has a custom backend plugin registered (see `backend_plugin.rs` / `GHOSTLINK_OPENAI_COMPAT_BASE_URL` in the main project), select it the same way as any built-in backend:

```ts
await client.settings.update({ inference_backend: "my_custom_backend" });
const resp = await client.chat.completions.create({ model: "anything", messages: [...] });
```

## API surface at a glance

| Python (`sdks/python`)                | JS/TS (this package)                    |
| -------------------------------------- | ---------------------------------------- |
| `client.chat.completions.create(...)`  | `client.chat.completions.create({...})`  |
| `client.completions.create(...)`       | `client.completions.create({...})`       |
| `client.embeddings.create(...)`        | `client.embeddings.create({...})`        |
| `client.models.list()`                 | `client.models.list()`                   |
| `client.list_models()`                 | `client.listModels()`                    |
| `client.workers.list()` / `.discover()` / `.connect()` / `.add()` / `.disconnect()` | same, unchanged |
| `client.sessions.list()`               | `client.sessions.list()`                 |
| `client.settings.get()` / `.update()` / `.reset()` | same, unchanged |
| `client.health()` / `.metrics()` / `.metrics_prometheus()` | `client.health()` / `.metrics()` / `.metricsPrometheus()` |
| `client.studio_chat(...)`              | `client.studioChat(...)`                 |
| `client.stream_chat(...)` (generator)  | `client.streamChat(...)` (async generator) |

Method and namespace names match the Python client one-for-one; only casing follows each language's convention (`snake_case` in Python, `camelCase` in JS/TS) and multi-argument calls take a single options object instead of keyword arguments.

## Node vs. browser

This SDK is built on the standard `fetch`, `ReadableStream`, and `TextDecoder` APIs — no HTTP client dependency (no axios) — so it runs unmodified in Node 18+ and in any modern browser. The one caveat: calling it directly from a browser page means the API key ships in that page's JS, so for browser use put this behind your own backend (or a proxy that injects the key) rather than embedding a Ghostlink API key in client-side code.

## Development

```bash
npm install
npm run build       # emits dist/ (ESM + CJS + .d.ts)
npm run type-check
npm test
```
