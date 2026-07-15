import { z } from 'zod';

export const AppConfigSchema = z.object({
  inference_backend: z.enum(['native', 'ollama']).default('native'),
  native_engine: z.enum(['llama_server', 'llama_cpp', 'simulated']).default('llama_server'),
  ngl: z.number().int().min(-1).max(200).default(0),
  model_path: z.string().default(''),
  llama_server_url: z.string().url().or(z.literal('')).default('http://127.0.0.1:8080/completion'),
  llama_port: z.number().int().min(1024).max(65535).default(8080),
  api_host: z.string().regex(/^[\w.*:-]+$/).default('127.0.0.1'),
  api_port: z.number().int().min(1024).max(65535).default(8003),
  gui_port: z.number().int().min(1024).max(65535).default(5173),
  threads: z.number().int().min(1).max(32).default(4),
  ctx_size: z.number().int().min(512).max(32768).step(512).default(4096),
  temperature: z.number().min(0).max(2).step(0.05).default(0.7),
  top_p: z.number().min(0).max(1).step(0.05).default(0.9),
  top_k: z.number().int().min(1).max(200).default(40),
  repeat_penalty: z.number().min(0).max(2).step(0.05).default(1.1),
  max_tokens: z.number().int().min(16).max(8192).step(16).default(512),
  chat_exec_tokens: z.number().int().min(64).max(8192).step(64).default(512),
  chat_micro_batch: z.number().int().min(1).max(32).default(1),
  tcp_max_inflight: z.number().int().min(16).max(4096).step(16).default(256),
  discovery_listen: z.string().default('0.0.0.0:8811'),
  discovery_broadcast: z.string().default('255.255.255.255:8811'),
  discovery_auth_token: z.string().default(''),
  tcp_auth_token: z.string().default(''),
  xdp_interface: z.string().default(''),
});

export type AppConfig = z.infer<typeof AppConfigSchema>;

export const defaultConfig: AppConfig = {
  inference_backend: 'native',
  native_engine: 'llama_server',
  ngl: 0,
  model_path: '',
  llama_server_url: 'http://127.0.0.1:8080/completion',
  llama_port: 8080,
  api_host: '127.0.0.1',
  api_port: 8003,
  gui_port: 5173,
  threads: 4,
  ctx_size: 4096,
  temperature: 0.7,
  top_p: 0.9,
  top_k: 40,
  repeat_penalty: 1.1,
  max_tokens: 512,
  chat_exec_tokens: 512,
  chat_micro_batch: 1,
  tcp_max_inflight: 256,
  discovery_listen: '0.0.0.0:8811',
  discovery_broadcast: '255.255.255.255:8811',
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
  
  // Check for required environment variables
  const apiBase = envVars.VITE_GHOSTLINK_API_BASE;
  if (apiBase) {
    try {
      new URL(apiBase);
    } catch {
      errors.push(`VITE_GHOSTLINK_API_BASE must be a valid URL, got: ${apiBase}`);
    }
  }
  
  return { valid: errors.length === 0, errors };
}

export const RuntimeSettingsSchema = z.object({
  inference_backend: z.string(),
  native_engine: z.string(),
  ngl: z.number(),
  model_path: z.string(),
  llama_server_url: z.string(),
  llama_port: z.number(),
  api_host: z.string(),
  api_port: z.number(),
  gui_port: z.number(),
  threads: z.number(),
  ctx_size: z.number(),
  temperature: z.number(),
  top_p: z.number(),
  top_k: z.number(),
  repeat_penalty: z.number(),
  max_tokens: z.number(),
  chat_exec_tokens: z.number(),
  chat_micro_batch: z.number(),
  tcp_max_inflight: z.number(),
  discovery_listen: z.string(),
  discovery_broadcast: z.string(),
  discovery_auth_token: z.string(),
  tcp_auth_token: z.string(),
  xdp_interface: z.string(),
});