import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ModelsTab } from './ModelsTab';
import { useAppStore } from '../store';
import { GhostlinkAPI } from '../api';

let setCurrentModelMock = vi.fn();

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
  vi.spyOn(api, 'getOllamaModels').mockResolvedValue({
    models: [
      { name: 'llama-3-8b', size: 8 * 1024 * 1024 * 1024, details: { family: 'llama', quantization_level: 'Q4_K_M' } },
      { name: 'mistral-7b', size: 7 * 1024 * 1024 * 1024, details: { family: 'mistral', quantization_level: 'Q8_0' } },
    ],
  });
  vi.spyOn(api, 'searchHuggingFace').mockResolvedValue({ models: [{ id: 'test/model', name: 'Test Model', downloads: 1000, likes: 50 }] });
  return api;
}

describe('ModelsTab', () => {
  beforeEach(() => {
    setCurrentModelMock = vi.fn();
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
      setCurrentModel: setCurrentModelMock,
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
    expect(screen.getByText('Popular Ollama Models')).toBeInTheDocument();
  });

  it('shows Ollama tab by default', () => {
    const api = createMockApi();
    render(<ModelsTab api={api} />);
    expect(screen.getAllByText('Ollama Models').length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText('Popular Ollama Models')).toBeInTheDocument();
  });

  it('switches to Hugging Face tab', () => {
    const api = createMockApi();
    render(<ModelsTab api={api} />);
    fireEvent.click(screen.getByText('Hugging Face'));
    expect(screen.getByText('Hugging Face Models')).toBeInTheDocument();
  });

  it('shows Use button for non-active models', async () => {
    const api = createMockApi();
    render(<ModelsTab api={api} />);
    fireEvent.click(screen.getByText('Refresh'));
    const useButtons = await screen.findAllByText('Use');
    expect(useButtons.length).toBeGreaterThanOrEqual(1);
  });

  it('shows Active badge for current model', async () => {
    const api = createMockApi();
    render(<ModelsTab api={api} />);
    fireEvent.click(screen.getByText('Refresh'));
    expect(await screen.findByText('Active')).toBeInTheDocument();
  });

  it('calls setCurrentModel when Use clicked', async () => {
    const api = createMockApi();
    render(<ModelsTab api={api} />);
    fireEvent.click(screen.getByText('Refresh'));
    const useBtn = await screen.findAllByText('Use');
    fireEvent.click(useBtn[0]);
    await waitFor(() => {
      expect(setCurrentModelMock).toHaveBeenCalled();
    });
  });
});
