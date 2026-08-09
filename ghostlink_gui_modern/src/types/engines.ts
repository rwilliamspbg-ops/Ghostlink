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

