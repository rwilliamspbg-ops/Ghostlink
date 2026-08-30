import { create } from "zustand";

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
  native_engine?: string;
  llama_server_url?: string;
  vllm_base_url?: string;
  vllm_api_key?: string;
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
  ngl: number;
  ngl_auto: boolean;
  ctx_size: number;
  ctx_size_auto: boolean;
  threads: number;
  threads_auto: boolean;
  batch_size: number | null;
  ubatch_size: number | null;
  kv_cache_type: "f16" | "q8_0" | "q4_0" | null;
  flash_attention: boolean;
  mlock: boolean | null;
  no_mmap: boolean | null;
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

export interface WorkspaceEntry {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
}

export type McpTransport =
  | { transport: "stdio"; command: string; args: string[]; env: Record<string, string> }
  | { transport: "http"; url: string; headers: Record<string, string> };

export interface McpServer {
  name: string;
  slot: string;
  enabled: boolean;
  connected: boolean;
  tool_count: number;
  requires_confirmation: boolean;
  transport: McpTransport;
  timeout_secs: number;
}

export interface McpServerInput {
  name: string;
  slot: string;
  enabled: boolean;
  requires_confirmation: boolean;
  timeout_secs: number;
  transport: "stdio" | "http";
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  url?: string;
  headers?: Record<string, string>;
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
  role: "user" | "assistant";
  content: string;
  id: string;
  timestamp: string;
  model?: string;
  toolCalls?: ToolCallTrace[];
  pendingToolCall?: PendingToolCall;
  compareGroupId?: string;
  truncatedBefore?: boolean;
  isDivider?: boolean;
}

export interface SystemPromptPreset {
  id: string;
  name: string;
  prompt: string;
  isBuiltIn?: boolean;
}

export interface PromptTemplate {
  id: string;
  title: string;
  content: string;
}

export interface Thread {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  messages: ChatMessage[];
  model?: string;
  temperature?: number;
  maxTokens?: number;
  top_p?: number;
  penalty?: number;
  systemPrompt?: string;
  pinned?: boolean;
}

export const BUILTIN_PRESETS: SystemPromptPreset[] = [
  {
    id: "default",
    name: "Default Co-pilot",
    prompt: "You are a production-grade, kernel-bypass-aware system orchestration co-pilot.",
    isBuiltIn: true,
  },
  {
    id: "concise",
    name: "Concise",
    prompt: "You are a concise, direct assistant. Answer in as few words as necessary without filler or fluff.",
    isBuiltIn: true,
  },
  {
    id: "code",
    name: "Code Expert",
    prompt: "You are an expert software engineer. Write clean, idiomatic, self-documenting code with concise explanations.",
    isBuiltIn: true,
  },
];

export const BUILTIN_USER_PROMPTS: PromptTemplate[] = [
  { id: "explain-code", title: "Explain Code", content: "Please explain how this code works step by step:" },
  { id: "refactor-func", title: "Refactor Function", content: "Refactor this function to improve performance, readability, and type safety:" },
  { id: "write-tests", title: "Write Unit Tests", content: "Write comprehensive unit tests for the following code:" },
  { id: "summarize", title: "Summarize Text", content: "Summarize the key points of the following content in concise bullet points:" },
];

export interface DownloadProgressEntry {
  progress: number;
  bytesDownloaded?: number;
  totalBytes?: number;
}

export interface Toast {
  id: string;
  type: "success" | "error" | "info";
  message: string;
}

type Updater<T> = T | ((prev: T) => T);
function resolveUpdater<T>(updater: Updater<T>, prev: T): T {
  return typeof updater === "function" ? (updater as (prev: T) => T)(prev) : updater;
}

interface AppState {
  apiBase: string;
  backendOnline: boolean;
  currentModel: string;
  uptime: number;
  models: Model[];
  metrics: Metric | null;
  metricsHistory: MetricSample[];
  sessions: Session[];
  workers: Worker[];
  backends: BackendInfo[];
  currentBackend: string;
  backendStatus: BackendStatus | null;
  selectedModel: string | null;
  activeTab: number;
  mcpServers: McpServer[];

  // Thread, preset, and prompt state
  threads: Thread[];
  activeThreadId: string | null;
  presets: SystemPromptPreset[];
  userPrompts: PromptTemplate[];

  chatMessages: ChatMessage[];
  chatLoading: boolean;
  chatStreamingId: string | null;
  chatError: string | null;
  pendingModelActions: Record<string, string>;
  downloadProgress: Record<string, DownloadProgressEntry>;

  editorOpenPath: string | null;
  editorContent: string;
  editorOriginalContent: string;
  editorPendingDiff: { proposed: string } | null;
  setEditorOpenFile: (path: string, content: string) => void;
  setEditorContent: (content: string) => void;
  setEditorSaved: () => void;
  setEditorPendingDiff: (diff: { proposed: string } | null) => void;
  closeEditorFile: () => void;

  toasts: Toast[];
  addToast: (toast: Omit<Toast, "id">) => string;
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

  createThread: (initialMessages?: ChatMessage[], modelOverride?: string) => Thread;
  selectThread: (id: string) => void;
  updateActiveThread: (updater: (thread: Thread) => Thread) => void;
  renameThread: (id: string, title: string) => void;
  deleteThread: (id: string) => void;
  togglePinThread: (id: string) => void;
  addPreset: (preset: Omit<SystemPromptPreset, "id">) => void;
  deletePreset: (id: string) => void;
  addUserPrompt: (prompt: Omit<PromptTemplate, "id">) => void;
  deleteUserPrompt: (id: string) => void;

  setChatMessages: (updater: Updater<ChatMessage[]>) => void;
  setChatLoading: (loading: boolean) => void;
  setChatStreamingId: (id: string | null) => void;
  setChatError: (error: string | null) => void;
  setPendingModelActions: (updater: Updater<Record<string, string>>) => void;
  setDownloadProgress: (updater: Updater<Record<string, DownloadProgressEntry>>) => void;
}

const THREADS_STORAGE_KEY = "ghostlink-threads-v2";
const PRESETS_STORAGE_KEY = "ghostlink-presets-v1";
const PROMPTS_STORAGE_KEY = "ghostlink-prompts-v1";
const CHAT_STORAGE_KEY = "ghostlink-chat-messages";

function loadStoredThreads(): { threads: Thread[]; activeThreadId: string | null } {
  try {
    const raw = localStorage.getItem(THREADS_STORAGE_KEY);
    if (raw) {
      const parsed: Thread[] = JSON.parse(raw);
      if (Array.isArray(parsed) && parsed.length > 0) {
        return { threads: parsed, activeThreadId: parsed[0].id };
      }
    }
  } catch {
    /* fallback */
  }

  let legacyMsgs: ChatMessage[] = [];
  try {
    const raw = localStorage.getItem(CHAT_STORAGE_KEY);
    if (raw) legacyMsgs = JSON.parse(raw);
  } catch {
    /* fallback */
  }

  const defaultThread: Thread = {
    id: `thread_${Date.now()}`,
    title: legacyMsgs.length > 0 ? (legacyMsgs[0].content.slice(0, 30) || "Previous Chat") : "New Chat",
    createdAt: Date.now(),
    updatedAt: Date.now(),
    messages: legacyMsgs,
  };

  return { threads: [defaultThread], activeThreadId: defaultThread.id };
}

function loadStoredPresets(): SystemPromptPreset[] {
  try {
    const raw = localStorage.getItem(PRESETS_STORAGE_KEY);
    if (raw) {
      const custom: SystemPromptPreset[] = JSON.parse(raw);
      return [...BUILTIN_PRESETS, ...custom];
    }
  } catch {
    /* fallback */
  }
  return BUILTIN_PRESETS;
}

function loadStoredPrompts(): PromptTemplate[] {
  try {
    const raw = localStorage.getItem(PROMPTS_STORAGE_KEY);
    if (raw) {
      const custom: PromptTemplate[] = JSON.parse(raw);
      return [...BUILTIN_USER_PROMPTS, ...custom];
    }
  } catch {
    /* fallback */
  }
  return BUILTIN_USER_PROMPTS;
}

function saveThreadsToStorage(threads: Thread[]) {
  try {
    localStorage.setItem(THREADS_STORAGE_KEY, JSON.stringify(threads.slice(0, 50)));
  } catch {
    /* quota */
  }
}

function savePresetsToStorage(presets: SystemPromptPreset[]) {
  try {
    const custom = presets.filter((p) => !p.isBuiltIn);
    localStorage.setItem(PRESETS_STORAGE_KEY, JSON.stringify(custom));
  } catch {
    /* quota */
  }
}

function savePromptsToStorage(prompts: PromptTemplate[]) {
  try {
    const custom = prompts.filter((p) => !BUILTIN_USER_PROMPTS.some((b) => b.id === p.id));
    localStorage.setItem(PROMPTS_STORAGE_KEY, JSON.stringify(custom));
  } catch {
    /* quota */
  }
}

const initialThreadData = loadStoredThreads();
const initialActiveThread = initialThreadData.threads.find((t) => t.id === initialThreadData.activeThreadId);

export const useAppStore = create<AppState>((set) => ({
  apiBase: "",
  backendOnline: false,
  currentModel: "none",
  uptime: 0,
  models: [],
  metrics: null,
  metricsHistory: [],
  sessions: [],
  workers: [],
  backends: [],
  currentBackend: "cpu",
  backendStatus: null,
  selectedModel: null,
  activeTab: 0,
  mcpServers: [],

  threads: initialThreadData.threads,
  activeThreadId: initialThreadData.activeThreadId,
  presets: loadStoredPresets(),
  userPrompts: loadStoredPrompts(),

  chatMessages: initialActiveThread ? initialActiveThread.messages : [],
  chatLoading: false,
  chatStreamingId: null,
  chatError: null,
  pendingModelActions: {},
  downloadProgress: {},
  toasts: [],
  editorOpenPath: null,
  editorContent: "",
  editorOriginalContent: "",
  editorPendingDiff: null,

  setApiBase: (base) => set({ apiBase: base }),
  setBackendOnline: (online) => set({ backendOnline: online }),
  setCurrentModel: (model) => set({ currentModel: model }),
  setUptime: (uptime) => set({ uptime }),
  setModels: (models) => set({ models }),
  setMetrics: (metrics) =>
    set((state) => ({
      metrics,
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

  createThread: (initialMessages = [], modelOverride) => {
    const newThread: Thread = {
      id: `thread_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`,
      title: "New Chat",
      createdAt: Date.now(),
      updatedAt: Date.now(),
      messages: initialMessages,
      model: modelOverride,
    };
    set((state) => {
      const threads = [newThread, ...state.threads];
      saveThreadsToStorage(threads);
      return {
        threads,
        activeThreadId: newThread.id,
        chatMessages: newThread.messages,
      };
    });
    return newThread;
  },

  selectThread: (id) =>
    set((state) => {
      const thread = state.threads.find((t) => t.id === id);
      if (!thread) return state;
      return {
        activeThreadId: id,
        chatMessages: thread.messages,
      };
    }),

  updateActiveThread: (updater) =>
    set((state) => {
      if (!state.activeThreadId) return state;
      const threads = state.threads.map((t) => {
        if (t.id === state.activeThreadId) {
          const updated = updater(t);
          return { ...updated, updatedAt: Date.now() };
        }
        return t;
      });
      const activeThread = threads.find((t) => t.id === state.activeThreadId);
      saveThreadsToStorage(threads);
      return {
        threads,
        chatMessages: activeThread ? activeThread.messages : state.chatMessages,
      };
    }),

  renameThread: (id, title) =>
    set((state) => {
      const threads = state.threads.map((t) => (t.id === id ? { ...t, title, updatedAt: Date.now() } : t));
      saveThreadsToStorage(threads);
      return { threads };
    }),

  deleteThread: (id) =>
    set((state) => {
      const threads = state.threads.filter((t) => t.id !== id);
      const activeThreadId =
        state.activeThreadId === id ? (threads.length > 0 ? threads[0].id : null) : state.activeThreadId;
      const activeThread = threads.find((t) => t.id === activeThreadId);
      saveThreadsToStorage(threads);
      return {
        threads,
        activeThreadId,
        chatMessages: activeThread ? activeThread.messages : [],
      };
    }),

  togglePinThread: (id) =>
    set((state) => {
      const threads = state.threads.map((t) => (t.id === id ? { ...t, pinned: !t.pinned } : t));
      saveThreadsToStorage(threads);
      return { threads };
    }),

  addPreset: (preset) =>
    set((state) => {
      const newPreset: SystemPromptPreset = {
        ...preset,
        id: `preset_${Date.now()}`,
        isBuiltIn: false,
      };
      const presets = [...state.presets, newPreset];
      savePresetsToStorage(presets);
      return { presets };
    }),

  deletePreset: (id) =>
    set((state) => {
      const presets = state.presets.filter((p) => p.id !== id || p.isBuiltIn);
      savePresetsToStorage(presets);
      return { presets };
    }),

  addUserPrompt: (prompt) =>
    set((state) => {
      const newPrompt: PromptTemplate = {
        ...prompt,
        id: `prompt_${Date.now()}`,
      };
      const userPrompts = [...state.userPrompts, newPrompt];
      savePromptsToStorage(userPrompts);
      return { userPrompts };
    }),

  deleteUserPrompt: (id) =>
    set((state) => {
      const userPrompts = state.userPrompts.filter((p) => p.id !== id);
      savePromptsToStorage(userPrompts);
      return { userPrompts };
    }),

  setChatMessages: (updater) =>
    set((state) => {
      const currentMsgs = state.chatMessages;
      const newMsgs = resolveUpdater(updater, currentMsgs);

      let activeThreadId = state.activeThreadId;
      let threads = state.threads;

      if (!activeThreadId || threads.length === 0) {
        const newThread: Thread = {
          id: `thread_${Date.now()}`,
          title: newMsgs.length > 0 ? (newMsgs[0].content.slice(0, 30) || "New Chat") : "New Chat",
          createdAt: Date.now(),
          updatedAt: Date.now(),
          messages: newMsgs,
        };
        threads = [newThread, ...threads];
        activeThreadId = newThread.id;
      } else {
        threads = threads.map((t) => {
          if (t.id === activeThreadId) {
            let title = t.title;
            if ((title === "New Chat" || title.startsWith("Session ")) && newMsgs.length > 0) {
              const firstUser = newMsgs.find((m) => m.role === "user");
              if (firstUser && firstUser.content) {
                title = firstUser.content.slice(0, 32).trim() || title;
              }
            }
            return {
              ...t,
              title,
              messages: newMsgs,
              updatedAt: Date.now(),
            };
          }
          return t;
        });
      }

      saveThreadsToStorage(threads);
      try {
        localStorage.setItem(CHAT_STORAGE_KEY, JSON.stringify(newMsgs.slice(-200)));
      } catch {
        /* quota */
      }

      return {
        threads,
        activeThreadId,
        chatMessages: newMsgs,
      };
    }),

  setChatLoading: (loading) => set({ chatLoading: loading }),
  setChatStreamingId: (id) => set({ chatStreamingId: id }),
  setChatError: (error) => set({ chatError: error }),
  setPendingModelActions: (updater) =>
    set((state) => ({ pendingModelActions: resolveUpdater(updater, state.pendingModelActions) })),
  setDownloadProgress: (updater) =>
    set((state) => ({ downloadProgress: resolveUpdater(updater, state.downloadProgress) })),
  setEditorOpenFile: (path, content) =>
    set({ editorOpenPath: path, editorContent: content, editorOriginalContent: content, editorPendingDiff: null }),
  setEditorContent: (content) => set({ editorContent: content }),
  setEditorSaved: () => set((state) => ({ editorOriginalContent: state.editorContent })),
  setEditorPendingDiff: (diff) => set({ editorPendingDiff: diff }),
  closeEditorFile: () =>
    set({ editorOpenPath: null, editorContent: "", editorOriginalContent: "", editorPendingDiff: null }),
  addToast: (toast) => {
    const id =
      typeof crypto !== "undefined" && crypto.randomUUID
        ? crypto.randomUUID()
        : `toast-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    set((state) => ({ toasts: [...state.toasts, { ...toast, id }] }));
    return id;
  },
  removeToast: (id) => set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) })),
}));
