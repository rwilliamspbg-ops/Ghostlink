/** TypeScript/JavaScript client SDK for the Ghostlink Studio API. */

export {
  GhostlinkClient,
  DEFAULT_TIMEOUT_MS,
  type GhostlinkClientOptions,
  type ChatMessage,
  type SamplingParams,
  type CreateChatCompletionParams,
  type CreateCompletionParams,
  type CreateEmbeddingsParams,
  type StudioChatParams,
} from "./client.js";

export {
  GhostlinkError,
  GhostlinkAPIError,
  GhostlinkAuthError,
  GhostlinkConnectionError,
} from "./exceptions.js";

export { ChatCompletion, Completion, Model, StreamChunk, type ChatChoice, type CompletionChoice, type JsonObject } from "./models.js";

export const VERSION = "0.1.0";
