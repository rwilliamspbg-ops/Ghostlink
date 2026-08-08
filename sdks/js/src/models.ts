/**
 * Typed response wrappers.
 *
 * Mirrors `sdks/python/ghostlink_client/models.py`: thin, permissive views
 * over the JSON the server returns. Every field is read with a default
 * rather than assumed present, so a minor server version skew (a new field
 * added, one renamed) doesn't throw inside the client. The original decoded
 * JSON is always available on `.raw` for anything not modeled explicitly
 * here.
 */

/* eslint-disable @typescript-eslint/no-explicit-any */

export type JsonObject = Record<string, any>;

function asString(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

function asNumber(value: unknown, fallback = 0): number {
  return typeof value === "number" ? value : fallback;
}

function asBool(value: unknown, fallback = false): boolean {
  return typeof value === "boolean" ? value : Boolean(value ?? fallback);
}

export interface ChatChoice {
  index: number;
  message: JsonObject;
  finishReason: string | null;
}

function chatChoiceFromDict(data: JsonObject): ChatChoice {
  return {
    index: asNumber(data.index, 0),
    message: (data.message as JsonObject) ?? {},
    finishReason: data.finish_reason ?? null,
  };
}

/** `POST /v1/chat/completions` response. */
export class ChatCompletion {
  readonly id: string;
  readonly object: string;
  readonly created: number;
  readonly model: string;
  readonly choices: ChatChoice[];
  /** The original decoded JSON response. */
  readonly raw: JsonObject;

  constructor(fields: {
    id: string;
    object: string;
    created: number;
    model: string;
    choices: ChatChoice[];
    raw: JsonObject;
  }) {
    this.id = fields.id;
    this.object = fields.object;
    this.created = fields.created;
    this.model = fields.model;
    this.choices = fields.choices;
    this.raw = fields.raw;
  }

  /** The first choice's assistant text, or `""` if there are none. */
  get content(): string {
    if (this.choices.length === 0) return "";
    const content = this.choices[0].message?.content;
    return typeof content === "string" ? content : "";
  }

  static fromDict(data: JsonObject): ChatCompletion {
    return new ChatCompletion({
      id: asString(data.id),
      object: asString(data.object),
      created: asNumber(data.created),
      model: asString(data.model),
      choices: Array.isArray(data.choices) ? data.choices.map(chatChoiceFromDict) : [],
      raw: data,
    });
  }
}

export interface CompletionChoice {
  text: string;
  index: number;
  finishReason: string | null;
}

/** `POST /v1/completions` response. */
export class Completion {
  readonly id: string;
  readonly object: string;
  readonly created: number;
  readonly model: string;
  readonly choices: CompletionChoice[];
  readonly raw: JsonObject;

  constructor(fields: {
    id: string;
    object: string;
    created: number;
    model: string;
    choices: CompletionChoice[];
    raw: JsonObject;
  }) {
    this.id = fields.id;
    this.object = fields.object;
    this.created = fields.created;
    this.model = fields.model;
    this.choices = fields.choices;
    this.raw = fields.raw;
  }

  get text(): string {
    return this.choices.length > 0 ? this.choices[0].text : "";
  }

  static fromDict(data: JsonObject): Completion {
    return new Completion({
      id: asString(data.id),
      object: asString(data.object),
      created: asNumber(data.created),
      model: asString(data.model),
      choices: Array.isArray(data.choices)
        ? data.choices.map((c: JsonObject) => ({
            text: asString(c.text),
            index: asNumber(c.index),
            finishReason: c.finish_reason ?? null,
          }))
        : [],
      raw: data,
    });
  }
}

/** One entry from `GET /v1/models`. */
export class Model {
  readonly id: string;
  readonly raw: JsonObject;

  constructor(fields: { id: string; raw: JsonObject }) {
    this.id = fields.id;
    this.raw = fields.raw;
  }

  static fromDict(data: JsonObject): Model {
    return new Model({ id: asString(data.id), raw: data });
  }
}

/** One incremental piece of a `/api/inference/chat` streamed response. */
export class StreamChunk {
  readonly token: string;
  readonly requestId: string;
  readonly sessionId: string;
  readonly error: boolean;
  readonly done: boolean;
  readonly truncated: boolean;
  readonly raw: JsonObject;

  constructor(fields: {
    token: string;
    requestId: string;
    sessionId: string;
    error: boolean;
    done: boolean;
    truncated: boolean;
    raw: JsonObject;
  }) {
    this.token = fields.token;
    this.requestId = fields.requestId;
    this.sessionId = fields.sessionId;
    this.error = fields.error;
    this.done = fields.done;
    this.truncated = fields.truncated;
    this.raw = fields.raw;
  }

  static fromDict(data: JsonObject): StreamChunk {
    return new StreamChunk({
      token: asString(data.token),
      requestId: asString(data.request_id),
      sessionId: asString(data.session_id),
      error: asBool(data.error),
      done: asBool(data.done),
      truncated: asBool(data.truncated),
      raw: data,
    });
  }
}
