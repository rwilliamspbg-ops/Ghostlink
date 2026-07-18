import { describe, it, expect, beforeEach } from 'vitest';
import { useAppStore } from './store';

beforeEach(() => {
  useAppStore.setState({
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
  });
});

describe('AppStore', () => {
  it('starts with default values', () => {
    const state = useAppStore.getState();
    expect(state.apiBase).toBe('');
    expect(state.currentModel).toBe('none');
    expect(state.uptime).toBe(0);
    expect(state.models).toEqual([]);
    expect(state.activeTab).toBe(0);
  });

  it('setApiBase updates the api base URL', () => {
    useAppStore.getState().setApiBase('http://localhost:8003');
    expect(useAppStore.getState().apiBase).toBe('http://localhost:8003');
  });

  it('setModels updates model list', () => {
    const models = [{ name: 'test', size_gb: 1, type: 'LLM', quantization: 'Q4', status: 'Ready', usable: true }];
    useAppStore.getState().setModels(models);
    expect(useAppStore.getState().models).toEqual(models);
  });

  it('setCurrentModel updates current model', () => {
    useAppStore.getState().setCurrentModel('llama-3-8b');
    expect(useAppStore.getState().currentModel).toBe('llama-3-8b');
  });

  it('setActiveTab switches tabs', () => {
    useAppStore.getState().setActiveTab(3);
    expect(useAppStore.getState().activeTab).toBe(3);
  });

  it('setWorkers updates worker list', () => {
    const workers = [{ id: 'w1', host: '127.0.0.1', port: 8003, status: 'Connected', model: 't', threads: 4, load: 50 }];
    useAppStore.getState().setWorkers(workers);
    expect(useAppStore.getState().workers).toHaveLength(1);
  });

  it('setBackends updates backend list', () => {
    const backends = [{ name: 'rocm', device_name: 'AMD Radeon 860M', vram_gb: 14.2, compute_capability: 'gfx906', driver_version: 'ROCm 6.1', status: 'active' }];
    useAppStore.getState().setBackends(backends);
    expect(useAppStore.getState().backends).toEqual(backends);
  });

  it('setCurrentBackend updates active backend', () => {
    useAppStore.getState().setCurrentBackend('rocm');
    expect(useAppStore.getState().currentBackend).toBe('rocm');
  });

  it('setBackendStatus updates backend status', () => {
    const backendStatus = { name: 'rocm', device_name: 'AMD Radeon 860M', vram_gb: 14.2, status: 'active', health: 'healthy', utilization: 24.5, temperature: 45 };
    useAppStore.getState().setBackendStatus(backendStatus);
    expect(useAppStore.getState().backendStatus).toEqual(backendStatus);
  });

  it('setSessions updates session list', () => {
    const sessions = [{ id: 's1', model: 'test', status: 'Running', throughput: 100, latency: 50, tokens: 500 }];
    useAppStore.getState().setSessions(sessions);
    expect(useAppStore.getState().sessions).toHaveLength(1);
  });

  it('setMetrics updates metrics', () => {
    const metrics = { throughput: 100, cpu: 50, memory: 30, gpu: 20, latency_p50: 10, latency_p95: 20 };
    useAppStore.getState().setMetrics(metrics);
    expect(useAppStore.getState().metrics?.throughput).toBe(100);
  });

  it('setBackendOnline updates connectivity', () => {
    useAppStore.getState().setBackendOnline(true);
    expect(useAppStore.getState().backendOnline).toBe(true);
  });
});
