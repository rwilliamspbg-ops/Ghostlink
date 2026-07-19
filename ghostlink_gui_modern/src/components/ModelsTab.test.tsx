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
  vi.spyOn(api, 'getOllamaModels').mockResolvedValue({
    models: [
      { name: 'llama-3-8b', size: 8 * 1024 * 1024 * 1024, details: { family: 'llama', quantization_level: 'Q4_K_M' } },
      { name: 'mistral-7b', size: 7 * 1024 * 1024 * 1024, details: { family: 'mistral', quantization_level: 'Q8_0' } },
    ],
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

  it('renders model list', async () => {
    const api = createMockApi();
    render(<ModelsTab api={api} />);
    await waitFor(() => {
      expect(screen.getByText('llama-3-8b')).toBeInTheDocument();
      expect(screen.getByText('mistral-7b')).toBeInTheDocument();
    });
  });

  it('shows Ollama tab by default', async () => {
    const api = createMockApi();
    render(<ModelsTab api={api} />);
    await waitFor(() => {
      expect(api.getModels).toHaveBeenCalled();
      expect(screen.getAllByText('Ollama Models').length).toBeGreaterThanOrEqual(2);
      expect(screen.getByText('Popular Ollama Models')).toBeInTheDocument();
    });
  });

  it('switches to Hugging Face tab', async () => {
    const api = createMockApi();
    render(<ModelsTab api={api} />);
    await waitFor(() => {
      expect(api.getModels).toHaveBeenCalled();
    });
    fireEvent.click(screen.getByText('Hugging Face'));
    await waitFor(() => {
      expect(screen.getByText('Hugging Face Models')).toBeInTheDocument();
    });
  });

  it('shows Use button for non-loaded models', async () => {
    const api = createMockApi();
    render(<ModelsTab api={api} />);
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Use' })).toBeInTheDocument();
    });
  });

  it('shows Active badge for current model', async () => {
    const api = createMockApi();
    render(<ModelsTab api={api} />);
    await waitFor(() => {
      expect(screen.getByText('Active')).toBeInTheDocument();
    });
  });

  it('calls loadModel when Load clicked', async () => {
    const api = createMockApi();
    render(<ModelsTab api={api} />);
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Use' })).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole('button', { name: 'Use' }));
    await waitFor(() => {
      expect(api.loadModel).toHaveBeenCalled();
    });
  });
});
