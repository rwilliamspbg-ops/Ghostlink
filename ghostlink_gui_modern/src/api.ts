import axios, { AxiosInstance } from 'axios';
import { Model, Metric, Session, Worker, Settings } from './store';

type CircuitState = 'closed' | 'open' | 'half-open';

export interface CircuitBreakerState {
  failures: number;
  successes: number;
  state: CircuitState;
  lastFailureTime: number;
}

export class GhostlinkAPI {
  private http: AxiosInstance;
  private requestTimeout = [5000, 120000] as const;
  private circuitBreaker: CircuitBreakerState = {
    failures: 0,
    successes: 0,
    state: 'closed',
    lastFailureTime: 0,
  };

  constructor(apiBase: string) {
    this.http = axios.create({
      baseURL: apiBase.trim(),
      timeout: this.requestTimeout[1],
    });
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

  async getHealth() {
    try {
      const response = await this.http.get('/health');
      return { success: true, data: response.data };
    } catch (error: any) {
      return { success: false, error: error?.message ?? String(error) };
    }
  }

  async getModels(): Promise<{ models: Model[]; current_model: string; error?: string }> {
    try {
      const response = await this.http.get('/api/models');
      const models = (response.data.models || []).map((m: any) => {
        const rawStatus = (m.status || 'unknown').toString().trim();
        const normalizedStatus =
          rawStatus.toLowerCase() === 'loaded'
            ? 'Loaded'
            : rawStatus.toLowerCase() === 'ready'
            ? 'Ready'
            : rawStatus;
        const type = m.type || 'unknown';
        const usable = normalizedStatus === 'Ready' || normalizedStatus === 'Loaded';
        return {
          ...m,
          status: normalizedStatus,
          type,
          usable,
        };
      });
      return { models, current_model: response.data.current_model };
    } catch (error: any) {
      return { models: [], current_model: 'none', error: error.message };
    }
  }

  async loadModel(modelName: string) {
    try {
      const response = await this.http.post('/api/models/load', { model: modelName });
      return { success: true, data: response.data };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async downloadModel(modelId: string) {
    try {
      const response = await this.http.post('/api/models/download', { model_id: modelId });
      return { success: true, data: response.data };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async getDownloadProgress(modelId: string): Promise<{ progress: number; status: string; error?: string }> {
    try {
      const response = await this.http.get('/api/models/download/progress', { params: { model_id: modelId } });
      const rawProgress = Number(response.data?.progress ?? 0);
      const progress = Number.isFinite(rawProgress) ? Math.max(0, Math.min(1, rawProgress)) : 0;
      const status = String(response.data?.status ?? 'unknown');
      return { progress, status };
    } catch (error: any) {
      try {
        // Fallback for backends that expose aggregate status instead of a per-model progress endpoint.
        const statusResponse = await this.http.get('/api/models/status');
        const downloadingModels = statusResponse.data?.downloading_models;

        if (Array.isArray(downloadingModels)) {
          const isDownloading = downloadingModels.includes(modelId);
          return {
            progress: isDownloading ? 0 : 1,
            status: isDownloading ? 'downloading' : 'completed',
          };
        }

        if (downloadingModels && typeof downloadingModels === 'object') {
          const rawValue = Number((downloadingModels as Record<string, unknown>)[modelId] ?? 0);
          const progress = Number.isFinite(rawValue) ? Math.max(0, Math.min(1, rawValue)) : 0;
          return {
            progress,
            status: progress >= 1 ? 'completed' : 'downloading',
          };
        }

        return { progress: 0, status: 'unknown', error: error.message };
      } catch (fallbackError: any) {
        return {
          progress: 0,
          status: 'unknown',
          error: fallbackError.response?.data?.error || fallbackError.message || error.message,
        };
      }
    }
  }

  async deleteModel(modelName: string) {
    try {
      const response = await this.http.delete(`/api/models/${encodeURIComponent(modelName)}`);
      return { success: true, data: response.data };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async unloadModel(modelName: string) {
    try {
      const response = await this.http.post(`/api/models/${encodeURIComponent(modelName)}/unload`);
      return { success: true, data: response.data };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async searchHuggingFace(query: string) {
    try {
      const response = await this.http.get('/api/models/search/huggingface', { params: { q: query } });
      return { models: response.data.models || [] };
    } catch (error: any) {
      return {
        models: [
          { id: `meta-llama/Llama-3-${query}`, name: `Llama 3 ${query}`, downloads: 1000000, likes: 50000 },
          { id: `mistralai/Mistral-${query}`, name: `Mistral ${query}`, downloads: 800000, likes: 40000 },
        ],
      };
    }
  }

  async discoverPeers() {
    try {
      const response = await this.http.get('/api/workers/discover');
      return { success: true, count: response.data.count || 0 };
    } catch (error: any) {
      return { success: false, error: error.message };
    }
  }

  async disconnectWorker(workerId: string) {
    try {
      const response = await this.http.post(`/api/workers/${workerId}/disconnect`);
      return { success: true, data: response.data };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async sendMessage(
    payload: {
      message: string;
      model?: string;
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
        // Use the baseURL if it's not empty, otherwise default to current origin (Vite proxy)
        const url = this.http.defaults.baseURL
            ? `${this.http.defaults.baseURL}/api/inference/chat`
            : '/api/inference/chat';

        const response = await fetch(url, {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
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
                // Ignore incomplete JSON
              }
            }
          }
        }

        return { success: true, data: { response: fullText } };
      } else {
        const response = await this.http.post('/api/inference/chat', payload);
        return { success: true, data: response.data };
      }
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async getMetrics(): Promise<{ metrics: Metric; error?: string }> {
    try {
      const response = await this.http.get('/api/metrics');
      return { metrics: response.data.metrics };
    } catch (error: any) {
      return { metrics: {} as Metric, error: error.message };
    }
  }

  async getSessions(): Promise<{ sessions: Session[]; error?: string }> {
    try {
      const response = await this.http.get('/api/sessions');
      return { sessions: response.data.sessions || [] };
    } catch (error: any) {
      return { sessions: [], error: error.message };
    }
  }

  async cancelSession(sessionId: string) {
    try {
      const response = await this.http.post(`/api/sessions/${sessionId}/cancel`);
      return { success: true, data: response.data };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async getRuntimes(): Promise<{ available_runtimes: any[]; error?: string }> {
    try {
      const response = await this.http.get('/api/runtime/detect');
      return { available_runtimes: response.data.available_runtimes || [] };
    } catch (error: any) {
      return { available_runtimes: [], error: error.message };
    }
  }

  async getSettings(): Promise<{ settings: Settings; error?: string }> {
    try {
      const response = await this.http.get('/api/settings');
      return { settings: response.data };
    } catch (error: any) {
      return { settings: {} as Settings, error: error.message };
    }
  }

  async updateSettings(settings: Partial<Settings>): Promise<{ success: boolean; settings?: Settings; error?: string }> {
    try {
      const response = await this.http.post('/api/settings', settings);
      return { success: true, settings: response.data.settings };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async resetSettings(): Promise<{ success: boolean; settings?: Settings; error?: string }> {
    try {
      const response = await this.http.post('/api/settings/reset');
      return { success: true, settings: response.data.settings };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async getWorkers(): Promise<{ workers: Worker[]; error?: string }> {
    try {
      const response = await this.http.get('/api/workers');
      return { workers: response.data.workers || [] };
    } catch (error: any) {
      return { workers: [], error: error.message };
    }
  }

  async addWorker(host: string, port: number): Promise<{ success: boolean; error?: string; data?: any }> {
    try {
      const response = await this.http.post('/api/workers/add', { host, port });
      return { success: true, data: response.data };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async selectRuntime(runtime: string): Promise<{ success: boolean; error?: string; data?: any }> {
    try {
      const response = await this.http.post('/api/runtime/select', { runtime });
      return { success: true, data: response.data };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async getBackends(): Promise<{ available: any[]; current: string; error?: string }> {
    try {
      const response = await this.http.get('/api/backends');
      return {
        available: response.data.available || [],
        current: response.data.current || 'cpu',
      };
    } catch (error: any) {
      return { available: [], current: 'cpu', error: error.response?.data?.error || error.message };
    }
  }

  async switchBackend(backend: string): Promise<{ success: boolean; restart_required?: boolean; error?: string; data?: any }> {
    try {
      const response = await this.http.post('/api/backends/switch', { backend });
      return {
        success: true,
        restart_required: !!response.data?.restart_required,
        data: response.data,
      };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.message || error.response?.data?.error || error.message };
    }
  }

  async getBackendStatus(name: string): Promise<{ status?: any; error?: string }> {
    try {
      const response = await this.http.get(`/api/backends/${encodeURIComponent(name)}/status`);
      return { status: response.data };
    } catch (error: any) {
      return { error: error.response?.data?.error || error.response?.data?.message || error.message };
    }
  }

  async getPQCState(): Promise<{ enabled: boolean; error?: string }> {
    try {
      const response = await this.http.get('/api/security/pqc/state');
      return { enabled: !!response.data?.enabled };
    } catch (error: any) {
      return { enabled: false, error: error.response?.data?.error || error.message };
    }
  }

  async enablePQC(): Promise<{ success: boolean; data?: any; error?: string }> {
    try {
      const response = await this.http.post('/api/security/pqc/enable');
      return { success: true, data: response.data };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async getAuditLog(): Promise<{ entries: any[]; error?: string }> {
    try {
      const response = await this.http.get('/api/security/audit-log');
      return { entries: response.data?.entries || [] };
    } catch (error: any) {
      return { entries: [], error: error.response?.data?.error || error.message };
    }
  }

  // Ollama-specific API methods
  async getOllamaHealth(): Promise<{ reachable: boolean; model_count: number; error?: string }> {
    try {
      const response = await this.http.get('/api/ollama/health');
      return { 
        reachable: response.data.reachable, 
        model_count: response.data.model_count 
      };
    } catch (error: any) {
      return { reachable: false, model_count: 0, error: error.message };
    }
  }

  async getOllamaModels(): Promise<{ models: any[]; error?: string }> {
    try {
      const response = await this.http.get('/api/ollama/models');
      return { models: response.data.models || [] };
    } catch (error: any) {
      return { models: [], error: error.message };
    }
  }

  async pullOllamaModel(modelName: string): Promise<{ success: boolean; error?: string }> {
    try {
      await this.http.post('/api/ollama/pull', { model: modelName });
      return { success: true };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async pullOllamaModelStream(modelName: string, onProgress: (progress: any) => void): Promise<{ success: boolean; error?: string }> {
    try {
      const response = await fetch(`${this.http.defaults.baseURL}/api/ollama/pull/stream`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ model: modelName }),
      });

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }

      const reader = response.body?.getReader();
      if (!reader) throw new Error('No response body');

      const decoder = new TextDecoder();
      let buffer = '';

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() || '';

        for (const line of lines) {
          if (line.startsWith('data: ')) {
            try {
              const data = JSON.parse(line.slice(6));
              onProgress(data);
            } catch (e) {
              // Ignore parse errors
            }
          }
        }
      }
      return { success: true };
    } catch (error: any) {
      return { success: false, error: error.message };
    }
  }

  async showOllamaModel(modelName: string): Promise<{ info: any; error?: string }> {
    try {
      const response = await this.http.post('/api/ollama/show', { model: modelName });
      return { info: response.data };
    } catch (error: any) {
      return { info: null, error: error.response?.data?.error || error.message };
    }
  }

  async createOllamaModel(name: string, modelfile: string): Promise<{ success: boolean; error?: string }> {
    try {
      await this.http.post('/api/ollama/create', { name, modelfile });
      return { success: true };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async copyOllamaModel(source: string, destination: string): Promise<{ success: boolean; error?: string }> {
    try {
      await this.http.post('/api/ollama/copy', { source, destination });
      return { success: true };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async deleteOllamaModel(name: string): Promise<{ success: boolean; error?: string }> {
    try {
      await this.http.post('/api/ollama/delete', { name });
      return { success: true };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async getOllamaRunningModels(): Promise<{ models: any[]; error?: string }> {
    try {
      const response = await this.http.get('/api/ollama/ps');
      return { models: response.data.models || [] };
    } catch (error: any) {
      return { models: [], error: error.message };
    }
  }

  async getOllamaEmbeddings(model: string, prompt: string): Promise<{ embedding: number[]; error?: string }> {
    try {
      const response = await this.http.post('/api/ollama/embeddings', { model, prompt });
      return { embedding: response.data.embedding || [] };
    } catch (error: any) {
      return { embedding: [], error: error.response?.data?.error || error.message };
    }
  }

  async getOllamaVersion(): Promise<{ version: string; error?: string }> {
    try {
      const response = await this.http.get('/api/ollama/version');
      return { version: response.data.version };
    } catch (error: any) {
      return { version: 'unknown', error: error.message };
    }
  }

  async chatOllama(model: string, messages: any[], options: any = {}): Promise<{ response: any; error?: string }> {
    try {
      const response = await this.http.post('/api/ollama/chat', { 
        model, 
        messages, 
        stream: options.stream || false,
        temperature: options.temperature,
        top_p: options.top_p,
        top_k: options.top_k,
        repeat_penalty: options.repeat_penalty,
        max_tokens: options.max_tokens,
      });
      return { response: response.data };
    } catch (error: any) {
      return { response: null, error: error.response?.data?.error || error.message };
    }
  }
}
