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

    it('should reject invalid native_engine', () => {
      const result = validateConfig({ ...defaultConfig, native_engine: 'invalid' });
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.errors.some(e => e.includes('native_engine'))).toBe(true);
      }
    });

    it('should reject ngl out of range', () => {
      const result = validateConfig({ ...defaultConfig, ngl: 300 });
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.errors.some(e => e.includes('ngl'))).toBe(true);
      }
    });

    it('should reject negative ngl less than -1', () => {
      const result = validateConfig({ ...defaultConfig, ngl: -5 });
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.errors.some(e => e.includes('ngl'))).toBe(true);
      }
    });

    it('should accept ngl = -1 (all layers)', () => {
      const result = validateConfig({ ...defaultConfig, ngl: -1 });
      expect(result.success).toBe(true);
    });

    it('should reject invalid llama_server_url', () => {
      const result = validateConfig({ ...defaultConfig, llama_server_url: 'not-a-url' });
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.errors.some(e => e.includes('llama_server_url'))).toBe(true);
      }
    });

    it('should accept empty llama_server_url', () => {
      const result = validateConfig({ ...defaultConfig, llama_server_url: '' });
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

    it('should reject ctx_size not multiple of 512', () => {
      const result = validateConfig({ ...defaultConfig, ctx_size: 513 });
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.errors.some(e => e.includes('ctx_size'))).toBe(true);
      }
    });

    it('should accept valid ctx_size multiples', () => {
      for (const size of [512, 1024, 2048, 4096, 8192, 16384, 32768]) {
        const result = validateConfig({ ...defaultConfig, ctx_size: size });
        expect(result.success).toBe(true);
      }
    });

    it('should reject negative threads', () => {
      const result = validateConfig({ ...defaultConfig, threads: 0 });
      expect(result.success).toBe(false);
    });

    it('should accept valid settings object', () => {
      const customConfig = {
        ...defaultConfig,
        inference_backend: 'ollama',
        native_engine: 'llama_cpp',
        ngl: 20,
        temperature: 0.8,
        top_p: 0.95,
        ctx_size: 8192,
        threads: 8,
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
        expect(result.data.native_engine).toBe('llama_server'); // default
        expect(result.data.ngl).toBe(0); // default
      }
    });
  });
});