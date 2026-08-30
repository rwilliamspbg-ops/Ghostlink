import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import App from './App';
import { useAppStore } from './store';

vi.mock('./api', () => ({
  GhostlinkAPI: vi.fn().mockImplementation(() => ({
    getModels: vi.fn().mockResolvedValue({ models: [], current_model: 'none' }),
    getHealth: vi.fn().mockResolvedValue({ success: true, data: {} }),
    getMetrics: vi.fn().mockResolvedValue({ metrics: { throughput: 0, cpu: 0, memory: 0, gpu: 0, latency_p50: 0, latency_p95: 0 } }),
    getSessions: vi.fn().mockResolvedValue({ sessions: [] }),
    listSessions: vi.fn().mockResolvedValue({ sessions: [] }),
    getWorkers: vi.fn().mockResolvedValue({ workers: [] }),
    listMcpServers: vi.fn().mockResolvedValue({ servers: [] }),
    getSettings: vi.fn().mockResolvedValue({ settings: { conversation_token_limit: 3072 } }),
  })),
}));

describe('App', () => {
  let mockSetApiBase: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    mockSetApiBase = vi.fn();
    useAppStore.setState({
      apiBase: 'http://localhost:8003',
      backendOnline: false,
      currentModel: 'none',
      uptime: 0,
      models: [],
      metrics: null,
      sessions: [],
      workers: [],
      selectedModel: null,
      activeTab: 0,
      setApiBase: mockSetApiBase,
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

  it('initializes API base on app load', () => {
    render(<App />);
    expect(mockSetApiBase).toHaveBeenCalledWith('http://127.0.0.1:8000');
  });

  it('renders the app with sidebar', () => {
    render(<App />);
    expect(screen.getByText('Ghostlink')).toBeInTheDocument();
  });

  it('shows all navigation tabs', () => {
    render(<App />);
    expect(screen.getByText('Chat')).toBeInTheDocument();
    expect(screen.getByText('Models')).toBeInTheDocument();
    expect(screen.getByText('Metrics')).toBeInTheDocument();
    expect(screen.getByText('Sessions')).toBeInTheDocument();
    expect(screen.getByText('Workers')).toBeInTheDocument();
    expect(screen.getByText('Security')).toBeInTheDocument();
    expect(screen.getByText('Settings')).toBeInTheDocument();
  });

  it('shows New Chat button', () => {
    render(<App />);
    expect(screen.getAllByText('New Chat')[0]).toBeInTheDocument();
  });
});
