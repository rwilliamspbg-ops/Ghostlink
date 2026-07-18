import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { SettingsTab } from './SettingsTab';
import { useAppStore } from '../store';

function createMockApi() {
  return {
    getSettings: vi.fn().mockResolvedValue({
      settings: {
        inference_backend: 'ollama',
        api_host: '127.0.0.1',
        api_port: 8003,
        gui_port: 5173,
        temperature: 0.7,
        top_p: 0.9,
        top_k: 40,
        repeat_penalty: 1.1,
        max_tokens: 1024,
        chat_exec_tokens: 512,
        chat_micro_batch: 4,
        tcp_max_inflight: 64,
        discovery_listen: '0.0.0.0',
        discovery_broadcast: '255.255.255.255',
        discovery_auth_token: '',
        tcp_auth_token: '',
        xdp_interface: 'eth0',
      },
    }),
    updateSettings: vi.fn(),
    resetSettings: vi.fn(),
    getBackends: vi.fn().mockResolvedValue({
      available: [
        {
          name: 'rocm',
          device_name: 'AMD Radeon 860M',
          vram_gb: 14.2,
          compute_capability: 'gfx906',
          driver_version: 'ROCm 6.1',
          status: 'active',
        },
        {
          name: 'cpu',
          device_name: 'AMD Ryzen AI 7 350',
          vram_gb: null,
          compute_capability: 'generic',
          driver_version: 'native',
          status: 'ready',
        },
      ],
      current: 'rocm',
    }),
    getBackendStatus: vi.fn().mockResolvedValue({
      status: {
        name: 'rocm',
        device_name: 'AMD Radeon 860M',
        vram_gb: 14.2,
        status: 'active',
        health: 'healthy',
        utilization: 24.5,
        temperature: 45,
      },
    }),
    switchBackend: vi.fn().mockResolvedValue({ success: true, restart_required: false }),
  };
}

describe('SettingsTab', () => {
  beforeEach(() => {
    useAppStore.setState({
      apiBase: 'http://localhost:8003',
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
      activeTab: 4,
      setApiBase: vi.fn(),
      setBackendOnline: vi.fn(),
      setCurrentModel: vi.fn(),
      setUptime: vi.fn(),
      setModels: vi.fn(),
      setMetrics: vi.fn(),
      setSessions: vi.fn(),
      setWorkers: vi.fn(),
      setBackends: vi.fn(),
      setCurrentBackend: vi.fn(),
      setBackendStatus: vi.fn(),
      setSelectedModel: vi.fn(),
      setActiveTab: vi.fn(),
    });
  });

  it('renders the compute backend section with current backend details', async () => {
    const api = createMockApi();

    render(<SettingsTab api={api} />);

    expect(screen.getByText('Loading settings...')).toBeInTheDocument();

    await waitFor(() => {
      expect(screen.getByText('Compute Backend')).toBeInTheDocument();
      expect(screen.getByText('Current Backend')).toBeInTheDocument();
      expect(screen.getAllByText('AMD Radeon 860M').length).toBeGreaterThanOrEqual(1);
      expect(screen.getByText('healthy')).toBeInTheDocument();
    });

    expect(api.getBackends).toHaveBeenCalled();
    expect(api.getBackendStatus).toHaveBeenCalledWith('rocm');
  });

  it('switches backend when the switch button is clicked', async () => {
    const api = createMockApi();

    render(<SettingsTab api={api} />);

    await waitFor(() => {
      expect(screen.getByText('cpu')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole('button', { name: /switch/i }));

    await waitFor(() => {
      expect(api.switchBackend).toHaveBeenCalledWith('cpu');
    });
  });
});