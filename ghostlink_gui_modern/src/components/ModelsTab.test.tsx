import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ModelsTab } from './ModelsTab';
import { useAppStore } from '../store';
import { GhostlinkAPI } from '../api';

function createMockApi(): GhostlinkAPI {
  const api = new GhostlinkAPI('http://localhost:8003');
  vi.spyOn(api, 'getModels').mockResolvedValue({
    models: [
      { name: 'llama-3-8b', size_gb: 8, type: 'LLM', quantization: 'Q4_K_M', status: 'Ready', usable: false },
      { name: 'mistral-7b', size_gb: 7, type: 'LLM', quantization: 'Q8_0', status: 'Loaded', usable: true },
    ],
    current_model: 'mistral-7b',
  });
  vi.spyOn(api, 'loadModel').mockResolvedValue({ success: true, data: {} });
  vi.spyOn(api, 'unloadModel').mockResolvedValue({ success: true, data: {} });
  vi.spyOn(api, 'deleteModel').mockResolvedValue({ success: true, data: {} });
  vi.spyOn(api, 'searchHuggingFace').mockResolvedValue({ models: [{ id: 'test/model', name: 'Test Model', downloads: 1000, likes: 50 }] });
  return api;
}

describe('ModelsTab', () => {
  beforeEach(() => {
    useAppStore.setState({
      currentModel: 'mistral-7b',
      models: [
        { name: 'llama-3-8b', size_gb: 8, type: 'LLM', quantization: 'Q4_K_M', status: 'Ready', usable: false },
        { name: 'mistral-7b', size_gb: 7, type: 'LLM', quantization: 'Q8_0', status: 'Loaded', usable: true },
      ],
      apiBase: 'http://localhost:8003',
      backendOnline: false,
      uptime: 0,
      metrics: null,
      sessions: [],
      workers: [],
      selectedModel: null,
      activeTab: 1,
      setApiBase: vi.fn(),
      setBackendOnline: vi.fn(),
      setCurrentModel: vi.fn(),
      setUptime: vi.fn(),
      setModels: vi.fn(),
      setMetrics: vi.fn(),
      setSessions: vi.fn(),
      setWorkers: vi.fn(),
      setSelectedModel: vi.fn(),
      setActiveTab: vi.fn(),
    });
  });

  it('renders model list', () => {
    const api = createMockApi();
    render(<ModelsTab api={api} />);
    expect(screen.getByText('llama-3-8b')).toBeInTheDocument();
    expect(screen.getByText('mistral-7b')).toBeInTheDocument();
  });

  it('shows Library tab by default', () => {
    const api = createMockApi();
    render(<ModelsTab api={api} />);
    expect(screen.getByText('Library')).toBeInTheDocument();
    expect(screen.getByText('Local Models')).toBeInTheDocument();
  });

  it('switches to Hugging Face tab', () => {
    const api = createMockApi();
    render(<ModelsTab api={api} />);
    fireEvent.click(screen.getByText('Hugging Face'));
    expect(screen.getByText('Hugging Face Models')).toBeInTheDocument();
  });

  it('shows Load button for non-loaded models', () => {
    const api = createMockApi();
    render(<ModelsTab api={api} />);
    const loadButtons = screen.getAllByText('Load');
    expect(loadButtons.length).toBeGreaterThanOrEqual(1);
  });

  it('shows Unload button for loaded model', () => {
    const api = createMockApi();
    render(<ModelsTab api={api} />);
    expect(screen.getByText('Unload')).toBeInTheDocument();
  });

  it('calls loadModel when Load clicked', async () => {
    const api = createMockApi();
    render(<ModelsTab api={api} />);
    const loadBtn = screen.getAllByText('Load')[0];
    fireEvent.click(loadBtn);
    await waitFor(() => {
      expect(api.loadModel).toHaveBeenCalled();
    });
  });
});
