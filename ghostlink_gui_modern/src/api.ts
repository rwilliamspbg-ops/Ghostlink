import axios, { AxiosInstance, AxiosError, InternalAxiosRequestConfig, AxiosResponse } from 'axios';
import { Model, Metric, Session, Worker, Settings } from './store';

export interface CircuitBreakerState {
  failures: number;
  successes: number;
  state: 'closed' | 'open' | 'half-open';
  lastFailureTime: number;
}

interface PendingRequest<T> {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason: any) => void;
  timestamp: number;
}

const CIRCUIT_BREAKER_THRESHOLD = 5;
const CIRCUIT_BREAKER_RESET_TIMEOUT = 30000;
const CIRCUIT_BREAKER_HALF_OPEN_SUCCESS_THRESHOLD = 2;
const REQUEST_DEDUP_WINDOW = 5000;

function isRetryableError(error: AxiosError): boolean {
  if (!error.response) return true;
  const status = error.response.status;
  return status >= 500 || status === 429 || status === 408;
}

function isAxiosError(error: any): error is AxiosError {
  return error && error.isAxiosError === true;
}

export class GhostlinkAPI {
  private http: AxiosInstance;
  private circuitBreaker: CircuitBreakerState = {
    failures: 0,
    successes: 0,
    state: 'closed',
    lastFailureTime: 0,
  };
  private pendingRequests = new Map<string, PendingRequest<any>>();

  constructor(apiBase: string) {
    const baseURL = apiBase.trim();
    
    this.http = axios.create({
      baseURL: baseURL,
      timeout: 120000,
    });

    this.http.interceptors.request.use(
      (config: InternalAxiosRequestConfig) => {
        this.checkCircuitBreaker();
        return config;
      },
      (error) => Promise.reject(error)
    );

    this.http.interceptors.response.use(
      (response: AxiosResponse) => {
        this.onSuccess();
        return response;
      },
      async (error: AxiosError) => {
        this.onFailure();
        
        const config = error.config as any;
        if (config && config.__retryCount === undefined) {
          config.__retryCount = 0;
        }
        
        if (config && isRetryableError(error) && config.__retryCount < 3) {
          config.__retryCount++;
          const delay = Math.min(1000 * Math.pow(2, config.__retryCount), 10000);
          await new Promise(resolve => setTimeout(resolve, delay));
          return this.http.request(config);
        }
        
        return Promise.reject(error);
      }
    );
  }

  

  private checkCircuitBreaker(): void {
    const now = Date.now();
    
    if (this.circuitBreaker.state === 'open') {
      if (now - this.circuitBreaker.lastFailureTime > CIRCUIT_BREAKER_RESET_TIMEOUT) {
        this.circuitBreaker.state = 'half-open';
        this.circuitBreaker.successes = 0;
      } else {
        throw new Error('Circuit breaker is open - too many failures');
      }
    }
  }

  private onSuccess(): void {
    this.circuitBreaker.failures = 0;
    
    if (this.circuitBreaker.state === 'half-open') {
      this.circuitBreaker.successes++;
      if (this.circuitBreaker.successes >= CIRCUIT_BREAKER_HALF_OPEN_SUCCESS_THRESHOLD) {
        this.circuitBreaker.state = 'closed';
      }
    }
  }

  private onFailure(): void {
    this.circuitBreaker.failures++;
    this.circuitBreaker.lastFailureTime = Date.now();
    
    if (this.circuitBreaker.failures >= CIRCUIT_BREAKER_THRESHOLD) {
      this.circuitBreaker.state = 'open';
    }
  }

  async getHealth() {
    try {
      const response = await this.http.get('/health');
      return { success: true, data: response.data };
    } catch (error) {
      return { success: false, error: this.extractErrorMessage(error) };
    }
  }

  async getModels(): Promise<{ models: Model[]; current_model: string; error?: string }> {
    return this.dedupeRequest('GET:/api/models', async () => {
      try {
        const response = await this.http.get('/api/models');
        const models = (response.data.models || []).map((m: any) => {
          const status = m.status || 'unknown';
          const type = m.type || 'unknown';
          const usable = status === 'Ready' || status === 'Loaded';
          return { ...m, status, type, usable };
        });
        return { models, current_model: response.data.current_model };
      } catch (error) {
        return { models: [], current_model: 'none', error: this.extractErrorMessage(error) };
      }
    });
  }

  async loadModel(modelName: string) {
    return this.dedupeRequest(`POST:/api/models/load:${modelName}`, async () => {
      try {
        const response = await this.http.post('/api/models/load', { model: modelName });
        return { success: true, data: response.data };
      } catch (error) {
        return { success: false, error: this.extractErrorMessage(error) };
      }
    });
  }

  async downloadModel(modelId: string) {
    return this.dedupeRequest(`POST:/api/models/download:${modelId}`, async () => {
      try {
        const response = await this.http.post('/api/models/download', { model_id: modelId });
        return { success: true, data: response.data };
      } catch (error) {
        return { success: false, error: this.extractErrorMessage(error) };
      }
    });
  }

  async deleteModel(modelName: string) {
    return this.dedupeRequest(`DELETE:/api/models/${modelName}`, async () => {
      try {
        const response = await this.http.delete(`/api/models/${modelName}`);
        return { success: true, data: response.data };
      } catch (error) {
        return { success: false, error: this.extractErrorMessage(error) };
      }
    });
  }

  async unloadModel(modelName: string) {
    return this.dedupeRequest(`POST:/api/models/${modelName}/unload`, async () => {
      try {
        const response = await this.http.post(`/api/models/${modelName}/unload`);
        return { success: true, data: response.data };
      } catch (error) {
        return { success: false, error: this.extractErrorMessage(error) };
      }
    });
  }

  async searchHuggingFace(query: string) {
    return this.dedupeRequest(`GET:/api/models/search/huggingface:${query}`, async () => {
      try {
        const response = await this.http.get('/api/models/search/huggingface', { params: { q: query } });
        return { models: response.data.models || [] };
      } catch (error) {
        return { models: [] };
      }
    });
  }

  async discoverPeers() {
    return this.dedupeRequest('GET:/api/workers/discover', async () => {
      try {
        const response = await this.http.get('/api/workers/discover');
        return { success: true, count: response.data.count || 0 };
      } catch (error) {
        return { success: false, error: this.extractErrorMessage(error) };
      }
    });
  }

  async disconnectWorker(workerId: string) {
    return this.dedupeRequest(`POST:/api/workers/${workerId}/disconnect`, async () => {
      try {
        const response = await this.http.post(`/api/workers/${workerId}/disconnect`);
        return { success: true, data: response.data };
      } catch (error) {
        return { success: false, error: this.extractErrorMessage(error) };
      }
    });
  }

  async sendMessage(
    payload: {
      message: string;
      temperature: number;
      top_p: number;
      top_k: number;
      penalty: number;
      max_tokens: number;
      system_prompt: string;
      mcp?: object;
      stream?: boolean;
    },
    onToken?: (token: string) => void
  ) {
    try {
      if (payload.stream) {
        const url = this.http.defaults.baseURL
            ? `${this.http.defaults.baseURL}/api/inference/chat`
            : '/api/inference/chat';

        const response = await fetch(url, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(payload),
        });

        if (!response.ok) {
          const errorData = await response.json().catch(() => ({}));
          throw new Error(errorData.error || `HTTP error! status: ${response.status}`);
        }

        const reader = response.body?.getReader();
        if (!reader) throw new Error('Response body is null');

        const decoder = new TextDecoder();
        let fullText = '';

        while (true) {
          const { done, value } = await reader.read();
          if (done) break;

          const chunk = decoder.decode(value, { stream: true });
          const lines = chunk.split('\n');

          for (const line of lines) {
            if (line.startsWith('data: ')) {
              try {
                const data = JSON.parse(line.slice(6));
                if (data.token) {
                  fullText += data.token;
                  if (onToken) onToken(data.token);
                }
              } catch (e) {
              }
            }
          }
        }

        return { success: true, data: { response: fullText } };
      } else {
        const response = await this.http.post('/api/inference/chat', payload);
        return { success: true, data: response.data };
      }
    } catch (error) {
      return { success: false, error: this.extractErrorMessage(error) };
    }
  }

  async getMetrics(): Promise<{ metrics: Metric; error?: string }> {
    return this.dedupeRequest('GET:/api/metrics', async () => {
      try {
        const response = await this.http.get('/api/metrics');
        return { metrics: response.data.metrics };
      } catch (error) {
        return { metrics: {} as Metric, error: this.extractErrorMessage(error) };
      }
    });
  }

  async getSessions(): Promise<{ sessions: Session[]; error?: string }> {
    return this.dedupeRequest('GET:/api/sessions', async () => {
      try {
        const response = await this.http.get('/api/sessions');
        return { sessions: response.data.sessions || [] };
      } catch (error) {
        return { sessions: [], error: this.extractErrorMessage(error) };
      }
    });
  }

  async cancelSession(sessionId: string) {
    return this.dedupeRequest(`POST:/api/sessions/${sessionId}/cancel`, async () => {
      try {
        const response = await this.http.post(`/api/sessions/${sessionId}/cancel`);
        return { success: true, data: response.data };
      } catch (error) {
        return { success: false, error: this.extractErrorMessage(error) };
      }
    });
  }

  async getRuntimes(): Promise<{ available_runtimes: any[]; error?: string }> {
    return this.dedupeRequest('GET:/api/runtime/detect', async () => {
      try {
        const response = await this.http.get('/api/runtime/detect');
        return { available_runtimes: response.data.available_runtimes || [] };
      } catch (error) {
        return { available_runtimes: [], error: this.extractErrorMessage(error) };
      }
    });
  }

  async getSettings(): Promise<{ settings: Settings; error?: string }> {
    return this.dedupeRequest('GET:/api/settings', async () => {
      try {
        const response = await this.http.get('/api/settings');
        return { settings: response.data };
      } catch (error) {
        return { settings: {} as Settings, error: this.extractErrorMessage(error) };
      }
    });
  }

  async updateSettings(settings: Partial<Settings>): Promise<{ success: boolean; settings?: Settings; error?: string }> {
    return this.dedupeRequest(`POST:/api/settings:${JSON.stringify(settings)}`, async () => {
      this.checkCircuitBreaker();
      try {
        const response = await this.http.post('/api/settings', settings);
        return { success: true, settings: response.data.settings };
      } catch (error) {
        return { success: false, error: this.extractErrorMessage(error) };
      }
    });
  }

  async resetSettings(): Promise<{ success: boolean; settings?: Settings; error?: string }> {
    return this.dedupeRequest('POST:/api/settings/reset', async () => {
      this.checkCircuitBreaker();
      try {
        const response = await this.http.post('/api/settings/reset');
        return { success: true, settings: response.data.settings };
      } catch (error) {
        return { success: false, error: this.extractErrorMessage(error) };
      }
    });
  }

  async getWorkers(): Promise<{ workers: Worker[]; error?: string }> {
    return this.dedupeRequest('GET:/api/workers', async () => {
      this.checkCircuitBreaker();
      try {
        const response = await this.http.get('/api/workers');
        return { workers: response.data.workers || [] };
      } catch (error) {
        return { workers: [], error: this.extractErrorMessage(error) };
      }
    });
  }

  async getDownloadProgress(modelId: string): Promise<{ progress: number; status: string; error?: string }> {
    return this.dedupeRequest(`GET:/api/models/download/progress:${modelId}`, async () => {
      this.checkCircuitBreaker();
      try {
        const response = await this.http.get('/api/models/download/progress', { params: { model_id: modelId } });
        const p = response.data;
        return { progress: p.progress || 0, status: p.status || 'unknown' };
      } catch (error) {
        return { progress: 0, status: 'unknown', error: this.extractErrorMessage(error) };
      }
    });
  }

  async addWorker(host: string, port: number): Promise<{ success: boolean; error?: string }> {
    return this.dedupeRequest(`POST:/api/workers/add:${host}:${port}`, async () => {
      this.checkCircuitBreaker();
      try {
        await this.http.post('/api/workers/add', { host, port });
        return { success: true };
      } catch (error) {
        return { success: false, error: this.extractErrorMessage(error) };
      }
    });
  }

  async refreshJWT(): Promise<{ success: boolean; data?: { token: string }; error?: string }> {
    return this.dedupeRequest('POST:/api/security/refresh-jwt', async () => {
      this.checkCircuitBreaker();
      try {
        const response = await this.http.post('/api/security/refresh-jwt');
        return { success: true, data: response.data };
      } catch (error) {
        return { success: false, error: this.extractErrorMessage(error) };
      }
    });
  }

  async getPQCState(): Promise<{ enabled: boolean; error?: string }> {
    return this.dedupeRequest('GET:/api/security/pqc-state', async () => {
      this.checkCircuitBreaker();
      try {
        const response = await this.http.get('/api/security/pqc-state');
        return { enabled: response.data?.enabled || false };
      } catch (error) {
        return { enabled: false, error: this.extractErrorMessage(error) };
      }
    });
  }

  async enablePQC(): Promise<{ success: boolean; data?: any; error?: string }> {
    return this.dedupeRequest('POST:/api/security/pqc/enable', async () => {
      this.checkCircuitBreaker();
      try {
        const response = await this.http.post('/api/security/pqc/enable');
        return { success: true, data: response.data };
      } catch (error) {
        return { success: false, error: this.extractErrorMessage(error) };
      }
    });
  }

  async selectRuntime(runtime: string): Promise<{ success: boolean; error?: string; data?: any }> {
    return this.dedupeRequest(`POST:/api/runtime/select:${runtime}`, async () => {
      this.checkCircuitBreaker();
      try {
        const res = await this.http.post('/api/runtime/select', { runtime });
        return { success: true, data: res.data };
      } catch (error) {
        return { success: false, error: this.extractErrorMessage(error) };
      }
    });
  }

  async getAuditLog(): Promise<{ entries: any[]; error?: string }> {
    return this.dedupeRequest('GET:/api/security/audit-log', async () => {
      this.checkCircuitBreaker();
      try {
        const response = await this.http.get('/api/security/audit-log');
        return { entries: response.data?.entries || [] };
      } catch (error) {
        return { entries: [], error: this.extractErrorMessage(error) };
      }
    });
  }

  private async dedupeRequest<T>(key: string, fn: () => Promise<T>): Promise<T> {
    const now = Date.now();
    const existing = this.pendingRequests.get(key);
    
    if (existing && now - existing.timestamp < REQUEST_DEDUP_WINDOW) {
      return existing.promise;
    }

    let resolve: (value: T) => void;
    let reject: (reason: any) => void;
    const promise = new Promise<T>((res, rej) => {
      resolve = res;
      reject = rej;
    });

    this.pendingRequests.set(key, { promise, resolve: resolve!, reject: reject!, timestamp: now });

    try {
      const result = await fn();
      resolve!(result);
      return result;
    } catch (error) {
      reject!(error);
      throw error;
    } finally {
      setTimeout(() => this.pendingRequests.delete(key), REQUEST_DEDUP_WINDOW);
    }
  }

  private extractErrorMessage(error: any): string {
    if (isAxiosError(error)) {
      const data = error.response?.data as { error?: string } | undefined;
      return data?.error || error.message || 'Unknown error';
    }
    return error?.message || String(error) || 'Unknown error';
  }

  getCircuitBreakerState(): CircuitBreakerState {
    return { ...this.circuitBreaker };
  }

  resetCircuitBreaker(): void {
    this.circuitBreaker = {
      failures: 0,
      successes: 0,
      state: 'closed',
      lastFailureTime: 0,
    };
  }
}