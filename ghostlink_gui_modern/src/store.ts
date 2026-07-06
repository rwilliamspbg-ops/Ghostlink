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
  apiBase: 'http://127.0.0.1:8003',
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
