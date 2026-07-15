import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ChatTab } from './ChatTab';
import { useAppStore } from '../store';
import { GhostlinkAPI } from '../api';

function createMockApi(): GhostlinkAPI {
  const api = new GhostlinkAPI('http://localhost:8003');
  vi.spyOn(api, 'getModels').mockResolvedValue({ models: [], current_model: 'none' });
  vi.spyOn(api, 'sendMessage').mockResolvedValue({ success: true, data: { response: 'Hello from test' } });
  vi.spyOn(api, 'loadModel').mockResolvedValue({ success: true, data: {} });
  return api;
}

describe('ChatTab', () => {
  beforeEach(() => {
    useAppStore.setState({
      currentModel: 'none',
      models: [],
      apiBase: 'http://localhost:8003',
      backendOnline: false,
      uptime: 0,
      metrics: null,
      sessions: [],
      workers: [],
      selectedModel: null,
      activeTab: 0,
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

  it('renders the chat interface', () => {
    const api = createMockApi();
    render(<ChatTab api={api} />);
    expect(screen.getByText('How can I help you today?')).toBeInTheDocument();
  });

  it('shows model selector when no model loaded', () => {
    const api = createMockApi();
    render(<ChatTab api={api} />);
    expect(screen.getByText('Select Model')).toBeInTheDocument();
  });

  it('shows send button', () => {
    const api = createMockApi();
    render(<ChatTab api={api} />);
    const textarea = screen.getByPlaceholderText('Send a Message');
    expect(textarea).toBeInTheDocument();
  });

  it('allows typing a message', () => {
    const api = createMockApi();
    render(<ChatTab api={api} />);
    const textarea = screen.getByPlaceholderText('Send a Message');
    fireEvent.change(textarea, { target: { value: 'Hello world' } });
    expect(textarea).toHaveValue('Hello world');
  });

  it('shows model name when a model is loaded', () => {
    useAppStore.setState({ currentModel: 'llama-3-8b', models: [{ name: 'llama-3-8b', size_gb: 8, type: 'LLM', quantization: 'Q4', status: 'Loaded', usable: true }] });
    const api = createMockApi();
    render(<ChatTab api={api} />);
    expect(screen.getByText('llama-3-8b')).toBeInTheDocument();
  });
});
