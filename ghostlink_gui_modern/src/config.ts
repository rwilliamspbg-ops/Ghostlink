import { z } from 'zod';

const API_BASE_ENV_KEYS = [
  'VITE_GHOSTLINK_API_BASE',
  'VITE_GHOSTLINK_BACKEND_URL',
  'GHOSTLINK_API_BASE',
  'GHOSTLINK_BACKEND_URL',
] as const;

export const AppConfigSchema = z.object({
  inference_backend: z.enum(['native', 'ollama']).default('ollama'),
  api_host: z.string().regex(/^[\w.*:-]+$/).default('127.0.0.1'),
  api_port: z.number().int().min(1024).max(65535).default(8003),
  gui_port: z.number().int().min(1024).max(65535).default(5173),
  temperature: z.number().min(0).max(2).step(0.05).default(0.7),
  top_p: z.number().min(0).max(1).step(0.05).default(0.9),
  top_k: z.number().int().min(1).max(200).default(40),
  repeat_penalty: z.number().min(0).max(2).step(0.05).default(1.1),
  max_tokens: z.number().int().min(16).max(8192).step(16).default(2048),
  /** Token budget for conversation *history* sent to the model — distinct
   *  from `max_tokens`, which caps the length of a single response. */
  conversation_token_limit: z.number().int().min(256).max(16384).step(128).default(3072),
  chat_exec_tokens: z.number().int().min(64).max(8192).step(64).default(512),
  chat_micro_batch: z.number().int().min(1).max(32).default(1),
  tcp_max_inflight: z.number().int().min(16).max(4096).step(16).default(256),
  discovery_listen: z.string().default('0.0.0.0:45885'),
  discovery_broadcast: z.string().default('255.255.255.255:45885'),
  discovery_auth_token: z.string().default(''),
  tcp_auth_token: z.string().default(''),
  xdp_interface: z.string().default(''),
});

export type AppConfig = z.infer<typeof AppConfigSchema>;

export const defaultConfig: AppConfig = {
  inference_backend: 'ollama',
  api_host: '127.0.0.1',
  api_port: 8003,
  gui_port: 5173,
  temperature: 0.7,
  top_p: 0.9,
  top_k: 40,
  repeat_penalty: 1.1,
  max_tokens: 2048,
  conversation_token_limit: 3072,
  chat_exec_tokens: 512,
  chat_micro_batch: 1,
  tcp_max_inflight: 256,
  discovery_listen: '0.0.0.0:45885',
  discovery_broadcast: '255.255.255.255:45885',
  discovery_auth_token: '',
  tcp_auth_token: '',
  xdp_interface: '',
};

export function validateConfig(config: unknown): { success: true; config: AppConfig } | { success: false; errors: string[] } {
  const result = AppConfigSchema.safeParse(config);
  
  if (!result.success) {
    return {
      success: false,
      errors: result.error.issues.map((e: any) => `${e.path.join('.')}: ${e.message}`),
    };
  }
  
return { success: true, config: result.data };
}

export function validateEnvVars(env?: Record<string, string>): { valid: boolean; errors: string[] } {
  const errors: string[] = [];

  // Use provided env or fallback to import.meta.env
  const envVars = env || (import.meta as any).env || {};

  // Check the configured API base regardless of which launcher alias set it.
  const apiBase = resolveApiBase(envVars);
  const apiBaseKey = API_BASE_ENV_KEYS.find((key) => {
    const value = envVars[key];
    return typeof value === 'string' && value.trim().length > 0;
  });

  if (apiBaseKey) {
    try {
      new URL(apiBase);
    } catch {
      errors.push(`${apiBaseKey} must be a valid URL, got: ${apiBase}`);
    }
  }

  return { valid: errors.length === 0, errors };
}

export function resolveApiBase(env?: Record<string, string>): string {
  const envVars = env || (import.meta as any).env || {};

  for (const key of API_BASE_ENV_KEYS) {
    const value = envVars[key];
    if (typeof value === 'string' && value.trim().length > 0) {
      return value.trim();
    }
  }

  // The control-plane gateway (Go), not ghost-link directly — it proxies
  // everything through to ghost-link (and, going forward, is where new
  // non-performance-critical control-plane features get built: request
  // logging, rate limiting). Any explicit env var above still wins, so a
  // deployment that genuinely wants to talk to ghost-link directly can.
  return 'http://127.0.0.1:8000';
}

export const RuntimeSettingsSchema = z.object({
  inference_backend: z.string(),
  api_host: z.string(),
  api_port: z.number(),
  gui_port: z.number(),
  temperature: z.number(),
  top_p: z.number(),
  top_k: z.number(),
  repeat_penalty: z.number(),
  max_tokens: z.number(),
  conversation_token_limit: z.number(),
  chat_exec_tokens: z.number(),
  chat_micro_batch: z.number(),
  tcp_max_inflight: z.number(),
  discovery_listen: z.string(),
  discovery_broadcast: z.string(),
  discovery_auth_token: z.string(),
  tcp_auth_token: z.string(),
  xdp_interface: z.string(),
});