/**
 * Client for a Ghostlink Studio API server.
 *
 * Wraps both the OpenAI-compatible REST surface (`/v1/chat/completions`,
 * `/v1/completions`, `/v1/embeddings`, `/v1/models`) and Ghostlink's native
 * `/api/*` surface (workers, sessions, settings, metrics, and real
 * token-by-token streaming chat via `/api/inference/chat`).
 *
 * Mirrors `sdks/python/ghostlink_client/client.py` method-for-method and
 * endpoint-for-endpoint. Parameter names use camelCase (idiomatic
 * TypeScript) rather than the Python client's snake_case kwargs, but map
 * onto the exact same JSON wire fields.
 */

import { iterSseJson, streamLines, type JsonObject } from "./sse.js";
import { GhostlinkAPIError, GhostlinkAuthError, GhostlinkConnectionError } from "./exceptions.js";
import { ChatCompletion, Completion, Model, StreamChunk } from "./models.js";

export const DEFAULT_TIMEOUT_MS = 30_000;

export interface ChatMessage {
  role: string;
  content: string;
  [key: string]: unknown;
}

/** Shared sampling parameters accepted by the OpenAI-compatible endpoints. */
export interface SamplingParams {
  temperature?: number;
  topP?: number;
  topK?: number;
  penalty?: number;
  maxTokens?: number;
}

export interface CreateChatCompletionParams extends SamplingParams {
  model: string;
  messages: ChatMessage[];
}

export interface CreateCompletionParams extends SamplingParams {
  model: string;
  prompt: string;
}

export interface CreateEmbeddingsParams {
  model: string;
  input: string | string[];
}

export interface StudioChatParams extends SamplingParams {
  messages?: ChatMessage[];
  model?: string;
}

function addOptional(payload: JsonObject, fields: Record<string, unknown>): void {
  for (const [key, value] of Object.entries(fields)) {
    if (value !== undefined) payload[key] = value;
  }
}

function samplingParamsToPayload(params: SamplingParams): JsonObject {
  const payload: JsonObject = {};
  addOptional(payload, {
    temperature: params.temperature,
    top_p: params.topP,
    top_k: params.topK,
    penalty: params.penalty,
    max_tokens: params.maxTokens,
  });
  return payload;
}

function extractErrorMessage(body: unknown): string | undefined {
  if (body && typeof body === "object") {
    const error = (body as JsonObject).error;
    if (error && typeof error === "object") {
      const message = (error as JsonObject).message;
      if (typeof message === "string") return message;
    }
    if (typeof error === "string") return error;
  }
  return undefined;
}

class ChatCompletions {
  constructor(private readonly client: GhostlinkClient) {}

  /** `POST /v1/chat/completions` — OpenAI-compatible, non-streaming. */
  async create(params: CreateChatCompletionParams): Promise<ChatCompletion> {
    const payload: JsonObject = {
      model: params.model,
      messages: params.messages,
      ...samplingParamsToPayload(params),
    };
    const data = await this.client._request("POST", "/v1/chat/completions", payload);
    return ChatCompletion.fromDict(data);
  }
}

class Chat {
  readonly completions: ChatCompletions;
  constructor(client: GhostlinkClient) {
    this.completions = new ChatCompletions(client);
  }
}

class Completions {
  constructor(private readonly client: GhostlinkClient) {}

  /** `POST /v1/completions` — OpenAI's legacy completions endpoint. */
  async create(params: CreateCompletionParams): Promise<Completion> {
    const payload: JsonObject = {
      model: params.model,
      prompt: params.prompt,
      ...samplingParamsToPayload(params),
    };
    const data = await this.client._request("POST", "/v1/completions", payload);
    return Completion.fromDict(data);
  }
}

class Embeddings {
  constructor(private readonly client: GhostlinkClient) {}

  /** `POST /v1/embeddings`. */
  async create(params: CreateEmbeddingsParams): Promise<JsonObject> {
    return this.client._request("POST", "/v1/embeddings", {
      model: params.model,
      input: params.input,
    });
  }
}

class Models {
  constructor(private readonly client: GhostlinkClient) {}

  /** `GET /v1/models`. */
  async list(): Promise<Model[]> {
    const data = await this.client._request("GET", "/v1/models");
    const items = Array.isArray(data.data) ? (data.data as JsonObject[]) : [];
    return items.map((m) => Model.fromDict(m));
  }
}

class Workers {
  constructor(private readonly client: GhostlinkClient) {}

  async list(): Promise<JsonObject[]> {
    const data = await this.client._request("GET", "/api/workers");
    return Array.isArray(data.workers) ? (data.workers as JsonObject[]) : [];
  }

  /**
   * Triggers a UDP-broadcast + mDNS discovery sweep and registers any peers
   * found. See `GET /api/workers/discover`.
   */
  async discover(): Promise<JsonObject> {
    return this.client._request("GET", "/api/workers/discover");
  }

  async connect(host: string, port: number): Promise<JsonObject> {
    return this.client._request("POST", "/api/workers/connect", { host, port });
  }

  async add(fields: JsonObject): Promise<JsonObject> {
    return this.client._request("POST", "/api/workers/add", fields);
  }

  async disconnect(workerId: string): Promise<JsonObject> {
    return this.client._request("POST", `/api/workers/${workerId}/disconnect`);
  }
}

class Sessions {
  constructor(private readonly client: GhostlinkClient) {}

  async list(): Promise<JsonObject[]> {
    const data = await this.client._request("GET", "/api/sessions");
    return Array.isArray(data.sessions) ? (data.sessions as JsonObject[]) : [];
  }
}

class Settings {
  constructor(private readonly client: GhostlinkClient) {}

  async get(): Promise<JsonObject> {
    return this.client._request("GET", "/api/settings");
  }

  async update(fields: JsonObject): Promise<JsonObject> {
    return this.client._request("POST", "/api/settings", fields);
  }

  async reset(): Promise<JsonObject> {
    return this.client._request("POST", "/api/settings/reset");
  }
}

export interface GhostlinkClientOptions {
  apiKey?: string;
  timeoutMs?: number;
  /** Override `fetch` (for tests, or an alternate runtime implementation). */
  fetchImpl?: typeof fetch;
}

/**
 * Client for a Ghostlink Studio API server.
 *
 * @example
 * ```ts
 * const client = new GhostlinkClient("http://127.0.0.1:8003", { apiKey: "..." });
 * const resp = await client.chat.completions.create({
 *   model: "llama3.2:3b",
 *   messages: [{ role: "user", content: "hello" }],
 * });
 * console.log(resp.content);
 * ```
 */
export class GhostlinkClient {
  readonly baseUrl: string;
  readonly apiKey?: string;
  readonly timeoutMs: number;

  readonly chat: Chat;
  readonly completions: Completions;
  readonly embeddings: Embeddings;
  readonly models: Models;
  readonly workers: Workers;
  readonly sessions: Sessions;
  readonly settings: Settings;

  private readonly fetchImpl: typeof fetch;

  constructor(baseUrl: string, options: GhostlinkClientOptions = {}) {
    let normalizedBaseUrl = baseUrl;
    while (normalizedBaseUrl.endsWith("/")) {
      normalizedBaseUrl = normalizedBaseUrl.slice(0, -1);
    }
    this.baseUrl = normalizedBaseUrl;
    this.apiKey = options.apiKey;
    this.timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    this.fetchImpl = options.fetchImpl ?? fetch;

    this.chat = new Chat(this);
    this.completions = new Completions(this);
    this.embeddings = new Embeddings(this);
    this.models = new Models(this);
    this.workers = new Workers(this);
    this.sessions = new Sessions(this);
    this.settings = new Settings(this);
  }

  private headers(): Record<string, string> {
    const headers: Record<string, string> = { "Content-Type": "application/json" };
    if (this.apiKey) headers["Authorization"] = `Bearer ${this.apiKey}`;
    return headers;
  }

  /** Fetches with a total-duration timeout on the connect/response-headers phase. */
  private async fetchWithTimeout(url: string, init: RequestInit): Promise<Response> {
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), this.timeoutMs);
    try {
      return await this.fetchImpl(url, { ...init, signal: controller.signal });
    } catch (err) {
      throw new GhostlinkConnectionError(
        `failed to reach ${url}: ${err instanceof Error ? err.message : String(err)}`,
      );
    } finally {
      clearTimeout(timeoutId);
    }
  }

  private async raiseForStatus(resp: Response): Promise<void> {
    if (resp.status < 400) return;
    const text = await resp.text();
    let body: unknown = text;
    try {
      body = text ? JSON.parse(text) : undefined;
    } catch {
      // not JSON — keep the raw text as the body, matching the Python client.
    }
    const message = extractErrorMessage(body) ?? resp.statusText ?? "request failed";
    if (resp.status === 401 || resp.status === 403) {
      throw new GhostlinkAuthError(resp.status, message, body);
    }
    throw new GhostlinkAPIError(resp.status, message, body);
  }

  /** @internal used by the nested resource classes. */
  async _request(method: string, path: string, json?: unknown): Promise<JsonObject> {
    const url = `${this.baseUrl}${path}`;
    const resp = await this.fetchWithTimeout(url, {
      method,
      headers: this.headers(),
      body: json !== undefined ? JSON.stringify(json) : undefined,
    });

    await this.raiseForStatus(resp);

    const text = await resp.text();
    if (!text) return {};
    return JSON.parse(text) as JsonObject;
  }

  /** `GET /health` — unauthenticated liveness/version/backend info. */
  async health(): Promise<JsonObject> {
    return this._request("GET", "/health");
  }

  /**
   * `GET /api/metrics` — JSON metrics snapshot. See `metricsPrometheus` for
   * the Prometheus-exposition-format sibling.
   */
  async metrics(): Promise<JsonObject> {
    return this._request("GET", "/api/metrics");
  }

  /**
   * `GET /metrics` — the same snapshot as `metrics()`, formatted for a
   * Prometheus scrape config instead of JSON.
   */
  async metricsPrometheus(): Promise<string> {
    const url = `${this.baseUrl}/metrics`;
    const resp = await this.fetchWithTimeout(url, { method: "GET", headers: this.headers() });
    await this.raiseForStatus(resp);
    return resp.text();
  }

  /** Convenience alias for `client.models.list()`. */
  async listModels(): Promise<Model[]> {
    return this.models.list();
  }

  /**
   * Non-streaming call to Ghostlink Studio's native chat endpoint
   * (`POST /api/inference/chat`) — richer than the OpenAI-compatible routes
   * (session bookkeeping, MCP tool results, live metrics in the response),
   * but Ghostlink-specific rather than portable. Use `streamChat` for real
   * token-by-token streaming.
   */
  async studioChat(message: string, params: StudioChatParams = {}): Promise<JsonObject> {
    const payload: JsonObject = { message };
    if (params.messages !== undefined) payload.messages = params.messages;
    if (params.model !== undefined) payload.model = params.model;
    Object.assign(payload, samplingParamsToPayload(params));
    return this._request("POST", "/api/inference/chat", payload);
  }

  /**
   * Real token-by-token streaming via `POST /api/inference/chat`
   * (`stream: true`) over Server-Sent Events. Yields one `StreamChunk` per
   * token as the model produces it; the final chunk has `done: true`. This
   * is the only endpoint with genuine incremental streaming today — the
   * OpenAI-compatible `/v1/chat/completions` accepts a `stream` field but
   * does not yet act on it.
   *
   * Note: unlike the other request methods, the client's timeout is only
   * applied to the connect/response-headers phase here, not the full
   * duration of the stream — a long-running token stream would otherwise be
   * truncated by a total-duration timeout meant for quick JSON calls.
   *
   * @example
   * ```ts
   * for await (const chunk of client.streamChat("tell me a story")) {
   *   if (!chunk.done) process.stdout.write(chunk.token);
   * }
   * ```
   */
  async *streamChat(message: string, params: StudioChatParams = {}): AsyncGenerator<StreamChunk> {
    const payload: JsonObject = { message, stream: true };
    if (params.messages !== undefined) payload.messages = params.messages;
    if (params.model !== undefined) payload.model = params.model;
    Object.assign(payload, samplingParamsToPayload(params));

    const url = `${this.baseUrl}/api/inference/chat`;
    const resp = await this.fetchWithTimeout(url, {
      method: "POST",
      headers: this.headers(),
      body: JSON.stringify(payload),
    });

    await this.raiseForStatus(resp);

    if (!resp.body) {
      throw new GhostlinkConnectionError(`no response body from ${url}`);
    }

    for await (const chunkData of iterSseJson(streamLines(resp.body))) {
      yield StreamChunk.fromDict(chunkData);
    }
  }
}
