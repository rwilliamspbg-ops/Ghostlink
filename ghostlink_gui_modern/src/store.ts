import { create } from 'zustand';

export interface Model {
  name: string;
  size_gb: number;
  type: string;
  quantization: string;
  status: string;
  usable: boolean;
}

export interface Metric {
  throughput: number;
  cpu: number;
  memory: number;
  gpu: number;
  latency_p50: number;
  latency_p95: number;
}

export interface Session {
  id: string;
  model: string;
  status: string;
  throughput: number;
  latency: number;
  tokens: number;
}

export interface Settings {
  inference_backend: string;
  native_engine: string;
  ngl: number;
  model_path: string;
  llama_server_url: string;
  llama_port: number;
  api_host: string;
  api_port: number;
  gui_port: number;
  threads: number;
  ctx_size: number;
  temperature: number;
  top_p: number;
  top_k: number;
  repeat_penalty: number;
  max_tokens: number;
  chat_exec_tokens: number;
  chat_micro_batch: number;
  tcp_max_inflight: number;
  discovery_listen: string;
  discovery_broadcast: string;
  discovery_auth_token: string;
  tcp_auth_token: string;
  xdp_interface: string;
}

export interface Worker {
  id: string;
  host: string;
  port: number;
  status: string;
  model: string;
  threads: number;
  load: number;
}

interface AppState {
  apiBase: string;
  backendOnline: boolean;
  currentModel: string;
  uptime: number;
  models: Model[];
  metrics: Metric | null;
  sessions: Session[];
  workers: Worker[];
  selectedModel: string | null;
  activeTab: number;
  
  setApiBase: (base: string) => void;
  setBackendOnline: (online: boolean) => void;
  setCurrentModel: (model: string) => void;
  setUptime: (uptime: number) => void;
  setModels: (models: Model[]) => void;
  setMetrics: (metrics: Metric) => void;
  setSessions: (sessions: Session[]) => void;
  setWorkers: (workers: Worker[]) => void;
  setSelectedModel: (model: string | null) => void;
  setActiveTab: (tab: number) => void;
}

export const useAppStore = create<AppState>((set) => ({
  apiBase: '',
  backendOnline: false,
  currentModel: 'none',
  uptime: 0,
  models: [],
  metrics: null,
  sessions: [],
  workers: [],
  selectedModel: null,
  activeTab: 0,
  
  setApiBase: (base) => set({ apiBase: base }),
  setBackendOnline: (online) => set({ backendOnline: online }),
  setCurrentModel: (model) => set({ currentModel: model }),
  setUptime: (uptime) => set({ uptime }),
  setModels: (models) => set({ models }),
  setMetrics: (metrics) => set({ metrics }),
  setSessions: (sessions) => set({ sessions }),
  setWorkers: (workers) => set({ workers }),
  setSelectedModel: (model) => set({ selectedModel: model }),
  setActiveTab: (tab) => set({ activeTab: tab }),
}));
