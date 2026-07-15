import axios, { AxiosInstance } from 'axios';
import { Model, Metric, Session, Worker, Settings } from './store';

export class GhostlinkAPI {
  private http: AxiosInstance;
  private requestTimeout = [5000, 120000] as const;

  constructor(apiBase: string) {
    this.http = axios.create({
      baseURL: apiBase,
      timeout: this.requestTimeout[1],
    });
  }

  async getHealth() {
    try {
      const response = await this.http.get('/health');
      return { success: true, data: response.data };
    } catch (error) {
      return { success: false, error: String(error) };
    }
  }

  async getModels(): Promise<{ models: Model[]; current_model: string; error?: string }> {
    try {
      const response = await this.http.get('/api/models');
      const models = (response.data.models || []).map((m: any) => {
        // Keep original status for consistent check with backend "Loaded"/"Ready"
        const status = m.status || 'unknown';
        const type = m.type || 'unknown';
        // Model is usable if status is Ready or Loaded
        const usable = status === 'Ready' || status === 'Loaded';
        return {
          ...m,
          status,
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

  async deleteModel(modelName: string) {
    try {
      const response = await this.http.delete(`/api/models/${modelName}`);
      return { success: true, data: response.data };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async unloadModel(modelName: string) {
    try {
      const response = await this.http.post(`/api/models/${modelName}/unload`);
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

  async getDownloadProgress(modelId: string): Promise<{ progress: number; status: string; error?: string }> {
    try {
      const response = await this.http.get('/api/models/download/progress', { params: { model_id: modelId } });
      const p = response.data;
      return { progress: p.progress || 0, status: p.status || 'unknown' };
    } catch (error: any) {
      return { progress: 0, status: 'unknown', error: error.message };
    }
  }

  async addWorker(host: string, port: number): Promise<{ success: boolean; error?: string }> {
    try {
      const response = await this.http.post('/api/workers/add', { host, port });
      return { success: true };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async refreshJWT(): Promise<{ success: boolean; data?: { token: string }; error?: string }> {
    try {
      const response = await this.http.post('/api/security/refresh-jwt');
      return { success: true, data: response.data };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async enablePQC(): Promise<{ success: boolean; data?: any; error?: string }> {
    try {
      const response = await this.http.post('/api/security/enable-pqc');
      return { success: true, data: response.data };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async selectRuntime(runtime: string): Promise<{ success: boolean; error?: string; data?: any }> {
    try {
      const res = await this.http.post('/api/runtime/select', { runtime });
      return { success: true, data: res.data };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async getAuditLog(): Promise<{ entries: any[]; error?: string }> {
    try {
      const response = await this.http.get('/api/security/audit-log');
      return { entries: response.data?.entries || [] };
    } catch (error: any) {
      return { entries: [], error: error.message };
    }
  }
}
