import { describe, it, expect } from 'vitest';
import { validateConfig, validateEnvVars, AppConfigSchema, defaultConfig } from '../src/config';

describe('Config Validation', () => {
  describe('validateConfig', () => {
    it('should accept valid default config', () => {
      const result = validateConfig(defaultConfig);
      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.config).toEqual(defaultConfig);
      }
    });

    it('should reject invalid inference_backend', () => {
      const result = validateConfig({ ...defaultConfig, inference_backend: 'invalid' });
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.errors.some(e => e.includes('inference_backend'))).toBe(true);
      }
    });

    it('should reject top_k out of range', () => {
      const result = validateConfig({ ...defaultConfig, top_k: 999 });
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.errors.some(e => e.includes('top_k'))).toBe(true);
      }
    });

    it('should reject chat_micro_batch out of range', () => {
      const result = validateConfig({ ...defaultConfig, chat_micro_batch: 64 });
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.errors.some(e => e.includes('chat_micro_batch'))).toBe(true);
      }
    });

    it('should reject chat_exec_tokens not multiple of 64', () => {
      const result = validateConfig({ ...defaultConfig, chat_exec_tokens: 65 });
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.errors.some(e => e.includes('chat_exec_tokens'))).toBe(true);
      }
    });

    it('should accept valid chat_exec_tokens step', () => {
      const result = validateConfig({ ...defaultConfig, chat_exec_tokens: 512 });
      expect(result.success).toBe(true);
    });

    it('should reject repeat_penalty out of range', () => {
      const result = validateConfig({ ...defaultConfig, repeat_penalty: 3 });
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.errors.some(e => e.includes('repeat_penalty'))).toBe(true);
      }
    });

    it('should accept empty xdp_interface', () => {
      const result = validateConfig({ ...defaultConfig, xdp_interface: '' });
      expect(result.success).toBe(true);
    });

    it('should reject invalid api_host', () => {
      const result = validateConfig({ ...defaultConfig, api_host: 'invalid host!' });
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.errors.some(e => e.includes('api_host'))).toBe(true);
      }
    });

    it('should reject api_port out of range', () => {
      const result = validateConfig({ ...defaultConfig, api_port: 99999 });
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.errors.some(e => e.includes('api_port'))).toBe(true);
      }
    });

    it('should reject temperature out of range', () => {
      const result = validateConfig({ ...defaultConfig, temperature: 3 });
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.errors.some(e => e.includes('temperature'))).toBe(true);
      }
    });

    it('should reject top_p out of range', () => {
      const result = validateConfig({ ...defaultConfig, top_p: 1.5 });
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.errors.some(e => e.includes('top_p'))).toBe(true);
      }
    });

    it('should reject max_tokens not multiple of 16', () => {
      const result = validateConfig({ ...defaultConfig, max_tokens: 513 });
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.errors.some(e => e.includes('max_tokens'))).toBe(true);
      }
    });

    it('should accept valid max_tokens multiples', () => {
      for (const size of [512, 1024, 2048, 4096, 8192]) {
        const result = validateConfig({ ...defaultConfig, max_tokens: size });
        expect(result.success).toBe(true);
      }
    });

    it('should reject tcp_max_inflight minimum', () => {
      const result = validateConfig({ ...defaultConfig, tcp_max_inflight: 0 });
      expect(result.success).toBe(false);
    });

    it('should accept valid settings object', () => {
      const customConfig = {
        ...defaultConfig,
        inference_backend: 'ollama',
        temperature: 0.8,
        top_p: 0.95,
        top_k: 32,
        repeat_penalty: 1.05,
        max_tokens: 4096,
        chat_micro_batch: 4,
      };
      const result = validateConfig(customConfig);
      expect(result.success).toBe(true);
    });
  });

  describe('validateEnvVars', () => {
    it('should accept valid VITE_GHOSTLINK_API_BASE', () => {
      const result = validateEnvVars({ VITE_GHOSTLINK_API_BASE: 'http://127.0.0.1:8003' });
      expect(result.valid).toBe(true);
    });

    it('should reject invalid VITE_GHOSTLINK_API_BASE', () => {
      const result = validateEnvVars({ VITE_GHOSTLINK_API_BASE: 'not-a-url' });
      expect(result.valid).toBe(false);
      expect(result.errors.length).toBeGreaterThan(0);
    });

    it('should accept missing VITE_GHOSTLINK_API_BASE', () => {
      const result = validateEnvVars({});
      expect(result.valid).toBe(true);
    });
  });

  describe('AppConfigSchema', () => {
    it('should parse valid config', () => {
      const result = AppConfigSchema.safeParse(defaultConfig);
      expect(result.success).toBe(true);
    });

    it('should apply defaults for optional fields', () => {
      const partialConfig = { inference_backend: 'ollama' };
      const result = AppConfigSchema.safeParse(partialConfig);
      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.data.inference_backend).toBe('ollama');
        expect(result.data.api_port).toBe(8003);
        expect(result.data.top_k).toBe(40);
      }
    });
  });
});