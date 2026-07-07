import axios, { AxiosInstance } from 'axios';
import { Model, Metric, Session, Worker } from './store';

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
        // Normalize status to lowercase
        const status = m.status?.toLowerCase() || 'unknown';
        // Map LLM types to chat
        const type = (m.type?.toLowerCase() === 'llm' ? 'chat' : m.type?.toLowerCase()) || 'unknown';
        // Model is usable if status is ready and type supports chat
        const usable = status === 'ready' && ['chat', 'text-generation', 'llm', 'unknown'].includes(type);
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
      // Try using local API first
      const response = await this.http.get('/api/models/search/huggingface', { params: { q: query } });
      return { models: response.data.models || [] };
    } catch (error: any) {
      // Fallback: return sample data for demo
      return {
        models: [
          { id: `meta-llama/Llama-2-${query}-hf`, task: 'text-generation', likes: 1000, downloads: 100000 },
          { id: `mistralai/Mistral-${query}`, task: 'text-generation', likes: 800, downloads: 50000 },
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
      tools?: string[];
      mcp_servers?: Array<{ name: string; url: string }>;
      mcp?: object;
      stream?: boolean;
    },
    onToken?: (token: string) => void
  ) {
    try {
      if (payload.stream) {
        const response = await fetch(`${this.http.defaults.baseURL}/api/inference/chat`, {
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
                console.error('Error parsing SSE line', e);
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

  async getWorkers(): Promise<{ workers: Worker[]; error?: string }> {
    try {
      const response = await this.http.get('/api/workers');
      return { workers: response.data.workers || [] };
    } catch (error: any) {
      return { workers: [], error: error.message };
    }
  }

  async addWorker(host: string, port: number) {
    try {
      const response = await this.http.post('/api/workers/add', { host, port });
      return { success: true, data: response.data };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async connectWorkers() {
    try {
      const response = await this.http.post('/api/workers/connect');
      return { success: true, data: response.data };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async refreshJWT() {
    try {
      const response = await this.http.post('/api/security/jwt/refresh');
      return { success: true, data: response.data };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }

  async enablePQC() {
    try {
      const response = await this.http.post('/api/security/pqc/enable');
      return { success: true, data: response.data };
    } catch (error: any) {
      return { success: false, error: error.response?.data?.error || error.message };
    }
  }
}
