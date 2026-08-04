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
        conversation_token_limit: 3072,
        parallel_slots: 1,
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
    getOllamaHealth: vi.fn().mockResolvedValue({ reachable: true, model_count: 3 }),
    getVllmHealth: vi.fn().mockResolvedValue({ reachable: true, model_count: 2 }),
    getInferenceEngines: vi.fn().mockResolvedValue({
      current: 'ollama',
      engines: [
        {
          name: 'ollama',
          label: 'Ollama',
          status: 'active',
          default_base_url: 'http://127.0.0.1:11434',
          capabilities: {
            streaming: true,
            model_listing: true,
            model_load: true,
            model_unload: true,
            structured_outputs: false,
            tool_calls: false,
          },
        },
        {
          name: 'vllm',
          label: 'vLLM',
          status: 'ready',
          default_base_url: 'http://127.0.0.1:8000',
          capabilities: {
            streaming: true,
            model_listing: true,
            model_load: true,
            model_unload: false,
            structured_outputs: true,
            tool_calls: true,
          },
        },
      ],
    }),
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
      // Status shows as "Active" or "ready" in the new UI
      expect(screen.getByText('Active')).toBeInTheDocument();
    });

    expect(api.getBackends).toHaveBeenCalled();
    expect(api.getInferenceEngines).toHaveBeenCalled();
  });

  it('switches backend when the switch button is clicked', async () => {
    const api = createMockApi();

    render(<SettingsTab api={api} />);

    await waitFor(() => {
      expect(screen.getByText('cpu')).toBeInTheDocument();
    });

    // Click the cpu backend option (radiogroup entry; has the backend name in its accessible name)
    fireEvent.click(screen.getByRole('radio', { name: /cpu/i }));

    await waitFor(() => {
      expect(api.switchBackend).toHaveBeenCalledWith('cpu');
    });
  });

  it('renders capability-aware engine details from the API', async () => {
    const api = createMockApi();

    render(<SettingsTab api={api} />);

    await waitFor(() => {
      expect(screen.getByText('Inference Engine')).toBeInTheDocument();
      expect(screen.getByText('Selected Engine')).toBeInTheDocument();
      expect(screen.getByText('Streaming')).toBeInTheDocument();
      expect(screen.getByText('Default endpoint:')).toBeInTheDocument();
      expect(screen.getByText('Reachable · 3 models')).toBeInTheDocument();
    });

    fireEvent.change(screen.getByLabelText('Backend'), { target: { value: 'vllm' } });

    await waitFor(() => {
      expect(screen.getByLabelText('vLLM Base URL')).toBeInTheDocument();
      expect(screen.getByLabelText('vLLM API Key')).toBeInTheDocument();
      expect(screen.getByText('Structured outputs')).toBeInTheDocument();
      expect(screen.getAllByText('Supported').length).toBeGreaterThan(0);
      expect(screen.getByText('Reachable · 2 models')).toBeInTheDocument();
    });
  });

  it('prompts confirm and calls resetSettings when Reset is confirmed', async () => {
    const api = createMockApi();
    api.resetSettings.mockResolvedValue({ success: true, settings: {} });
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);

    render(<SettingsTab api={api} />);

    await waitFor(() => {
      expect(screen.getByText('Compute Backend')).toBeInTheDocument();
    });

    const resetBtn = screen.getByRole('button', { name: /reset/i });
    expect(resetBtn).toHaveAttribute('title', 'Reset all settings to defaults');
    fireEvent.click(resetBtn);

    expect(confirmSpy).toHaveBeenCalledWith(
      'Are you sure you want to reset all settings to defaults? This will overwrite your current configuration.'
    );
    expect(api.resetSettings).toHaveBeenCalled();
  });

  it('prompts confirm and does not call resetSettings when Reset is cancelled', async () => {
    const api = createMockApi();
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(false);

    render(<SettingsTab api={api} />);

    await waitFor(() => {
      expect(screen.getByText('Compute Backend')).toBeInTheDocument();
    });

    const resetBtn = screen.getByRole('button', { name: /reset/i });
    fireEvent.click(resetBtn);

    expect(confirmSpy).toHaveBeenCalled();
    expect(api.resetSettings).not.toHaveBeenCalled();
  });
});