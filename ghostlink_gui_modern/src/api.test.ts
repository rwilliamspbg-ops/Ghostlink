import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import axios from 'axios';
import { GhostlinkAPI } from './api';

vi.mock('axios');

describe('GhostlinkAPI', () => {
  let api: GhostlinkAPI;
  const mockAxiosInstance = {
    get: vi.fn(),
    post: vi.fn(),
    delete: vi.fn(),
    defaults: { baseURL: 'http://127.0.0.1:8003' },
    interceptors: {
      request: { use: vi.fn(), eject: vi.fn() },
      response: { use: vi.fn(), eject: vi.fn() },
    },
  };

  beforeEach(() => {
    vi.clearAllMocks();
    (axios.create as any).mockReturnValue(mockAxiosInstance);
    api = new GhostlinkAPI('http://127.0.0.1:8003');
  });

  afterEach(() => {
    api.resetCircuitBreaker();
  });

  describe('URL Validation', () => {
    it('should accept valid HTTP URL', () => {
      const api2 = new GhostlinkAPI('http://localhost:8003');
      expect(api2).toBeDefined();
    });

    it('should accept valid HTTPS URL', () => {
      const api2 = new GhostlinkAPI('https://api.example.com');
      expect(api2).toBeDefined();
    });

    it('should accept URL with path', () => {
      const api2 = new GhostlinkAPI('http://localhost:8003/api');
      expect(api2).toBeDefined();
    });

    it('should trim whitespace from URL', () => {
      const api2 = new GhostlinkAPI('  http://localhost:8003  ');
      expect(api2).toBeDefined();
    });
  });

  describe('Error Handling', () => {
    it('should return structured error on network failure', async () => {
      mockAxiosInstance.get.mockRejectedValue(new Error('Network Error'));
      
      const result = await api.getModels();
      
      expect(result).toEqual({
        models: [],
        current_model: 'none',
        error: 'Network Error',
      });
    });

    it('should return structured error on HTTP error response', async () => {
      const error: any = new Error('Server Error');
      error.isAxiosError = true;
      error.response = { data: { error: 'Model not found' }, status: 404 };
      mockAxiosInstance.post.mockRejectedValue(error);
      
      const result = await api.loadModel('nonexistent');
      
      expect(result.success).toBe(false);
      expect(result.error).toBe('Model not found');
    });

    it('should handle timeout errors gracefully', async () => {
      const error: any = new Error('timeout of 30000ms exceeded');
      error.code = 'ECONNABORTED';
      error.isAxiosError = true;
      mockAxiosInstance.post.mockRejectedValue(error);
      
      const result = await api.downloadModel('test-model');
      
      expect(result.success).toBe(false);
      expect(result.error).toContain('timeout');
    });
  });

  describe('Circuit Breaker', () => {
    it('should have initial closed state', () => {
      const state = api.getCircuitBreakerState();
      expect(state.state).toBe('closed');
      expect(state.failures).toBe(0);
    });

    it('should reset circuit breaker', async () => {
      const error: any = new Error('Server Error');
      error.isAxiosError = true;
      error.response = { status: 500 };
      mockAxiosInstance.get.mockRejectedValue(error);
      
      for (let i = 0; i < 6; i++) {
        await api.getModels();
      }
      
      api.resetCircuitBreaker();
      
      const state = api.getCircuitBreakerState();
      expect(state.state).toBe('closed');
      expect(state.failures).toBe(0);
    });
  });

  describe('Health Check', () => {
    it('should return success true on healthy', async () => {
      mockAxiosInstance.get.mockResolvedValue({ data: { status: 'ok' } });
      
      const result = await api.getHealth();
      
      expect(result.success).toBe(true);
      expect(result.data).toEqual({ status: 'ok' });
    });

    it('should return success false on error', async () => {
      mockAxiosInstance.get.mockRejectedValue(new Error('Connection refused'));
      
      const result = await api.getHealth();
      
      expect(result.success).toBe(false);
      expect(result.error).toBe('Connection refused');
    });
  });

  describe('Model Operations', () => {
    it('should fetch models successfully', async () => {
      mockAxiosInstance.get.mockResolvedValue({ 
        data: { 
          models: [{ name: 'test-model', size_gb: 1, type: 'LLM', quantization: 'Q4', status: 'Ready', usable: true }],
          current_model: 'test-model'
        } 
      });
      
      const result = await api.getModels();
      
      expect(result.models).toHaveLength(1);
      expect(result.current_model).toBe('test-model');
    });

    it('should load model successfully', async () => {
      mockAxiosInstance.post.mockResolvedValue({ data: { success: true } });
      
      const result = await api.loadModel('test-model');
      
      expect(result.success).toBe(true);
      expect(mockAxiosInstance.post).toHaveBeenCalledWith('/api/models/load', { model: 'test-model' });
    });

    it('should download model', async () => {
      mockAxiosInstance.post.mockResolvedValue({ data: { job_id: '123' } });
      
      const result = await api.downloadModel('model-id');
      
      expect(result.success).toBe(true);
      expect(mockAxiosInstance.post).toHaveBeenCalledWith('/api/models/download', { model_id: 'model-id' });
    });

    it('should delete model', async () => {
      mockAxiosInstance.delete.mockResolvedValue({ data: { success: true } });
      
      const result = await api.deleteModel('test-model');
      
      expect(result.success).toBe(true);
      expect(mockAxiosInstance.delete).toHaveBeenCalledWith('/api/models/test-model');
    });

    it('should unload model', async () => {
      mockAxiosInstance.post.mockResolvedValue({ data: { success: true } });
      
      const result = await api.unloadModel('test-model');
      
      expect(result.success).toBe(true);
      expect(mockAxiosInstance.post).toHaveBeenCalledWith('/api/models/test-model/unload');
    });

    it('should search HuggingFace', async () => {
      mockAxiosInstance.get.mockResolvedValue({ data: { models: [{ id: 'test/model', name: 'Test Model' }] } });
      
      const result = await api.searchHuggingFace('test');
      
      expect(result.models).toHaveLength(1);
      expect(mockAxiosInstance.get).toHaveBeenCalledWith('/api/models/search/huggingface', { params: { q: 'test' } });
    });
  });

  describe('Session Operations', () => {
    it('should fetch sessions', async () => {
      mockAxiosInstance.get.mockResolvedValue({ data: { sessions: [{ id: 's1', model: 'test', status: 'Running', throughput: 100, latency: 50, tokens: 500 }] } });
      
      const result = await api.getSessions();
      
      expect(result.sessions).toHaveLength(1);
    });

    it('should cancel session', async () => {
      mockAxiosInstance.post.mockResolvedValue({ data: { success: true } });
      
      const result = await api.cancelSession('s1');
      
      expect(result.success).toBe(true);
      expect(mockAxiosInstance.post).toHaveBeenCalledWith('/api/sessions/s1/cancel');
    });
  });

  describe('Worker Operations', () => {
    it('should fetch workers', async () => {
      mockAxiosInstance.get.mockResolvedValue({ data: { workers: [{ id: 'w1', host: '127.0.0.1', port: 8003, status: 'Connected', model: 'test', threads: 4, load: 50 }] } });
      
      const result = await api.getWorkers();
      
      expect(result.workers).toHaveLength(1);
    });

    it('should add worker', async () => {
      mockAxiosInstance.post.mockResolvedValue({ data: { success: true } });
      
      const result = await api.addWorker('192.168.1.100', 8003);
      
      expect(result.success).toBe(true);
      expect(mockAxiosInstance.post).toHaveBeenCalledWith('/api/workers/add', { host: '192.168.1.100', port: 8003 });
    });

    it('should discover peers', async () => {
      mockAxiosInstance.get.mockResolvedValue({ data: { count: 2 } });
      
      const result = await api.discoverPeers();
      
      expect(result.success).toBe(true);
      expect(result.count).toBe(2);
    });

    it('should disconnect worker', async () => {
      mockAxiosInstance.post.mockResolvedValue({ data: { success: true } });
      
      const result = await api.disconnectWorker('w1');
      
      expect(result.success).toBe(true);
      expect(mockAxiosInstance.post).toHaveBeenCalledWith('/api/workers/w1/disconnect');
    });
  });

  describe('Settings Operations', () => {
    it('should fetch settings', async () => {
      mockAxiosInstance.get.mockResolvedValue({ data: { api_port: 8003, gui_port: 5173 } });
      
      const result = await api.getSettings();
      
      expect(result.settings.api_port).toBe(8003);
    });

    it('should update settings', async () => {
      mockAxiosInstance.post.mockResolvedValue({ data: { settings: { api_port: 8004 } } });
      
      const result = await api.updateSettings({ api_port: 8004 });
      
      expect(result.success).toBe(true);
      expect(result.settings?.api_port).toBe(8004);
    });

    it('should reset settings', async () => {
      mockAxiosInstance.post.mockResolvedValue({ data: { settings: { api_port: 8003 } } });
      
      const result = await api.resetSettings();
      
      expect(result.success).toBe(true);
    });
  });

  describe('Runtime Operations', () => {
    it('should fetch runtimes', async () => {
      mockAxiosInstance.get.mockResolvedValue({ data: { available_runtimes: [{ runtime: 'cuda', is_available: true }] } });
      
      const result = await api.getRuntimes();
      
      expect(result.available_runtimes).toHaveLength(1);
    });

    it('should select runtime', async () => {
      mockAxiosInstance.post.mockResolvedValue({ data: { success: true } });
      
      const result = await api.selectRuntime('cuda');
      
      expect(result.success).toBe(true);
    });
  });

  describe('Security Operations', () => {
    it('should fetch PQC state', async () => {
      mockAxiosInstance.get.mockResolvedValue({ data: { enabled: true } });
      
      const result = await api.getPQCState();
      
      expect(result.enabled).toBe(true);
    });

    it('should enable PQC', async () => {
      mockAxiosInstance.post.mockResolvedValue({ data: { success: true } });
      
      const result = await api.enablePQC();
      
      expect(result.success).toBe(true);
    });

    it('should fetch audit log', async () => {
      mockAxiosInstance.get.mockResolvedValue({ data: { entries: [{ id: 1, action: 'test' }] } });
      
      const result = await api.getAuditLog();
      
      expect(result.entries).toHaveLength(1);
    });
  });

  describe('Metrics', () => {
    it('should fetch metrics', async () => {
      mockAxiosInstance.get.mockResolvedValue({ data: { metrics: { throughput: 100, cpu: 50, memory: 30, gpu: 20, latency_p50: 10, latency_p95: 20 } } });
      
      const result = await api.getMetrics();
      
      expect(result.metrics.throughput).toBe(100);
    });
  });

  describe('Chat Streaming', () => {
    it('should handle non-streaming response', async () => {
      mockAxiosInstance.post.mockResolvedValue({ data: { response: 'Hello World' } });
      
      const result = await api.sendMessage({
        message: 'test',
        temperature: 0.7,
        top_p: 0.9,
        top_k: 40,
        penalty: 1.1,
        max_tokens: 100,
        system_prompt: '',
        stream: false,
      });
      
      expect(result.success).toBe(true);
      expect(result.data?.response).toBe('Hello World');
    });
  });

  describe('Download Progress', () => {
    it('should fetch download progress', async () => {
      mockAxiosInstance.get.mockResolvedValue({ data: { progress: 0.5, status: 'downloading' } });
      
      const result = await api.getDownloadProgress('model-id');
      
      expect(result.progress).toBe(0.5);
      expect(result.status).toBe('downloading');
    });
  });
});