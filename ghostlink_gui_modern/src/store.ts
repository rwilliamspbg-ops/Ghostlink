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
  active_nodes?: number;
  total_vram_gb?: number;
  total_memory_gb?: number;
  used_memory_gb?: number;
  gpu_available?: boolean;
  real_inference?: boolean;
  samples?: number;
  last_latency_ms?: number;
  last_tokens?: number;
  uptime_s?: number;
  inference_backend?: string;
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
  api_host: string;
  api_port: number;
  gui_port: number;
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

export interface BackendInfo {
  name: string;
  device_name: string;
  vram_gb: number | null;
  compute_capability: string;
  driver_version: string;
  status: string;
}

export interface BackendStatus {
  name: string;
  device_name: string;
  vram_gb: number | null;
  status: string;
  health: string;
  utilization: number | null;
  temperature: number | null;
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
  backends: BackendInfo[];
  currentBackend: string;
  backendStatus: BackendStatus | null;
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
  setBackends: (backends: BackendInfo[]) => void;
  setCurrentBackend: (backend: string) => void;
  setBackendStatus: (status: BackendStatus | null) => void;
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
  backends: [],
  currentBackend: 'cpu',
  backendStatus: null,
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
  setBackends: (backends) => set({ backends }),
  setCurrentBackend: (currentBackend) => set({ currentBackend }),
  setBackendStatus: (backendStatus) => set({ backendStatus }),
  setSelectedModel: (model) => set({ selectedModel: model }),
  setActiveTab: (tab) => set({ activeTab: tab }),
}));
