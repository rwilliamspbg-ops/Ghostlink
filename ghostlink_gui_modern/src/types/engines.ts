export type InferenceEngineName = 'ollama' | 'native' | 'vllm';

export type InferenceEngineStatus = 'active' | 'ready';

export interface InferenceEngineCapabilities {
  streaming: boolean;
  model_listing: boolean;
  model_load: boolean;
  model_unload: boolean;
  structured_outputs: boolean;
  tool_calls: boolean;
}

export interface InferenceEngineDescriptor {
  name: InferenceEngineName;
  label: string;
  status: InferenceEngineStatus;
  default_base_url?: string | null;
  capabilities: InferenceEngineCapabilities;
}

const ENGINE_CAPABILITIES: Record<InferenceEngineName, InferenceEngineCapabilities> = {
  ollama: {
    streaming: true,
    model_listing: true,
    model_load: true,
    model_unload: true,
    structured_outputs: false,
    tool_calls: false,
  },
  native: {
    streaming: true,
    model_listing: false,
    model_load: true,
    model_unload: true,
    structured_outputs: false,
    tool_calls: false,
  },
  vllm: {
    streaming: true,
    model_listing: true,
    model_load: true,
    model_unload: false,
    structured_outputs: true,
    tool_calls: true,
  },
};

const ENGINE_LABELS: Record<InferenceEngineName, string> = {
  ollama: 'Ollama',
  native: 'Native',
  vllm: 'vLLM',
};

const ENGINE_BASE_URLS: Record<InferenceEngineName, string | null> = {
  ollama: 'http://127.0.0.1:11434',
  native: null,
  vllm: 'http://127.0.0.1:8000',
};

export function createInferenceEngineDescriptors(active: InferenceEngineName = 'ollama'): InferenceEngineDescriptor[] {
  return (['ollama', 'native', 'vllm'] as const).map((name) => ({
    name,
    label: ENGINE_LABELS[name],
    status: name === active ? 'active' : 'ready',
    default_base_url: ENGINE_BASE_URLS[name],
    capabilities: ENGINE_CAPABILITIES[name],
  }));
}