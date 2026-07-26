import { create } from 'zustand';

export interface Model {
  name: string;
  size_gb: number;
  type: string;
  quantization: string;
  status: string;
  usable: boolean;
}

export interface MetricSample extends Metric {
  /** Client-side capture time (ms epoch) — the backend metrics payload has no timestamp of its own. */
  t: number;
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
  conversation_token_limit: number;
  parallel_slots: number;
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

export interface McpServer {
  name: string;
  slot: string;
  enabled: boolean;
  connected: boolean;
  tool_count: number;
  requires_confirmation: boolean;
}

export interface ToolCallTrace {
  tool: string;
  server: string;
  result: string;
  success: boolean;
}

export interface PendingToolCall {
  request_id: string;
  tool: string;
  server: string;
  args: unknown;
}

export interface ChatMessage {
  role: 'user' | 'assistant';
  content: string;
  id: string;
  timestamp: string;
  model?: string;
  toolCalls?: ToolCallTrace[];
  pendingToolCall?: PendingToolCall;
  /** Set on both assistant replies from a single Compare Mode turn so the UI can render them side-by-side. */
  compareGroupId?: string;
  /** Server dropped some earlier history to fit conversation_token_limit before generating this reply — renders a divider above it so the gap is visible instead of looking like the model forgot on its own. */
  truncatedBefore?: boolean;
}

export interface DownloadProgressEntry {
  progress: number;
  bytesDownloaded?: number;
  totalBytes?: number;
}

export interface Toast {
  id: string;
  type: 'success' | 'error' | 'info';
  message: string;
}

type Updater<T> = T | ((prev: T) => T);
function resolveUpdater<T>(updater: Updater<T>, prev: T): T {
  return typeof updater === 'function' ? (updater as (prev: T) => T)(prev) : updater;
}

interface AppState {
  apiBase: string;
  backendOnline: boolean;
  currentModel: string;
  uptime: number;
  models: Model[];
  metrics: Metric | null;
  // Bounded trailing history of metrics samples, appended alongside setMetrics
  // (App.tsx polls every 3s) — powers the Metrics tab's sparklines so trends
  // are visible instead of just a current-value snapshot. Capped so it never
  // grows unbounded across a long session.
  metricsHistory: MetricSample[];
  sessions: Session[];
  workers: Worker[];
  backends: BackendInfo[];
  currentBackend: string;
  backendStatus: BackendStatus | null;
  selectedModel: string | null;
  activeTab: number;
  mcpServers: McpServer[];

  // Chat and model-download state live here (rather than component-local
  // useState) because the app only renders the active tab — switching tabs
  // unmounts ChatTab/ModelsTab. A streaming chat reply or a download's
  // progress poll loop is a plain async closure that keeps running
  // regardless of unmount, but its setState calls become no-ops against a
  // torn-down component. Storing the data here means it keeps updating in
  // the background and the tab picks up right where it left off on remount.
  chatMessages: ChatMessage[];
  chatLoading: boolean;
  chatStreamingId: string | null;
  chatError: string | null;
  pendingModelActions: Record<string, string>;
  downloadProgress: Record<string, DownloadProgressEntry>;

  // App-wide transient notifications (rendered by <Toaster/> in App.tsx).
  // Distinct from the persistent inline error/empty-state banners each tab
  // already has for context-tied validation/connection errors — this is for
  // one-off action feedback ("Saved", "Deleted", a background failure) that
  // should fade on its own rather than occupy permanent screen space.
  toasts: Toast[];
  addToast: (toast: Omit<Toast, 'id'>) => string;
  removeToast: (id: string) => void;

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
  setMcpServers: (servers: McpServer[]) => void;
  setChatMessages: (updater: Updater<ChatMessage[]>) => void;
  setChatLoading: (loading: boolean) => void;
  setChatStreamingId: (id: string | null) => void;
  setChatError: (error: string | null) => void;
  setPendingModelActions: (updater: Updater<Record<string, string>>) => void;
  setDownloadProgress: (updater: Updater<Record<string, DownloadProgressEntry>>) => void;
}

const CHAT_STORAGE_KEY = 'ghostlink-chat-messages';
function loadStoredChatMessages(): ChatMessage[] {
  try {
    const raw = localStorage.getItem(CHAT_STORAGE_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

export const useAppStore = create<AppState>((set) => ({
  apiBase: '',
  backendOnline: false,
  currentModel: 'none',
  uptime: 0,
  models: [],
  metrics: null,
  metricsHistory: [],
  sessions: [],
  workers: [],
  backends: [],
  currentBackend: 'cpu',
  backendStatus: null,
  selectedModel: null,
  activeTab: 0,
  mcpServers: [],
  chatMessages: loadStoredChatMessages(),
  chatLoading: false,
  chatStreamingId: null,
  chatError: null,
  pendingModelActions: {},
  downloadProgress: {},
  toasts: [],

  setApiBase: (base) => set({ apiBase: base }),
  setBackendOnline: (online) => set({ backendOnline: online }),
  setCurrentModel: (model) => set({ currentModel: model }),
  setUptime: (uptime) => set({ uptime }),
  setModels: (models) => set({ models }),
  setMetrics: (metrics) =>
    set((state) => ({
      metrics,
      // ~6 minutes of history at the 3s poll interval App.tsx uses — enough
      // for a meaningful trend chart without growing unbounded.
      metricsHistory: [...state.metricsHistory, { ...metrics, t: Date.now() }].slice(-120),
    })),
  setSessions: (sessions) => set({ sessions }),
  setWorkers: (workers) => set({ workers }),
  setBackends: (backends) => set({ backends }),
  setCurrentBackend: (currentBackend) => set({ currentBackend }),
  setBackendStatus: (backendStatus) => set({ backendStatus }),
  setSelectedModel: (model) => set({ selectedModel: model }),
  setActiveTab: (tab) => set({ activeTab: tab }),
  setMcpServers: (servers) => set({ mcpServers: servers }),
  setChatMessages: (updater) =>
    set((state) => {
      const chatMessages = resolveUpdater(updater, state.chatMessages);
      try {
        localStorage.setItem(CHAT_STORAGE_KEY, JSON.stringify(chatMessages.slice(-200)));
      } catch {
        /* quota */
      }
      return { chatMessages };
    }),
  setChatLoading: (loading) => set({ chatLoading: loading }),
  setChatStreamingId: (id) => set({ chatStreamingId: id }),
  setChatError: (error) => set({ chatError: error }),
  setPendingModelActions: (updater) =>
    set((state) => ({ pendingModelActions: resolveUpdater(updater, state.pendingModelActions) })),
  setDownloadProgress: (updater) =>
    set((state) => ({ downloadProgress: resolveUpdater(updater, state.downloadProgress) })),
  addToast: (toast) => {
    const id = typeof crypto !== 'undefined' && crypto.randomUUID
      ? crypto.randomUUID()
      : `toast-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    set((state) => ({ toasts: [...state.toasts, { ...toast, id }] }));
    return id;
  },
  removeToast: (id) => set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) })),
}));
