import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { MetricsTab } from './MetricsTab';
import { useAppStore } from '../store';

function createMockApi(overrides: Partial<Record<string, any>> = {}) {
  return {
    getMetrics: vi.fn().mockResolvedValue({
      metrics: {
        throughput: 128.5,
        cpu: 42,
        memory: 58,
        gpu: 71,
        latency_p50: 32,
        latency_p95: 84,
        active_nodes: 2,
        total_vram_gb: 16,
        total_memory_gb: 32,
        used_memory_gb: 18,
        gpu_available: true,
        real_inference: true,
        samples: 5,
        uptime_s: 600,
        inference_backend: 'vllm',
      },
    }),
    getMetricsHistory: vi.fn().mockResolvedValue({
      history: [
        { timestamp_ms: 1, throughput: 100, cpu: 30, memory: 50, gpu: 60, latency_p50: 40, latency_p95: 90, active_nodes: 2, inference_backend: 'vllm' },
        { timestamp_ms: 2, throughput: 120, cpu: 35, memory: 52, gpu: 62, latency_p50: 38, latency_p95: 86, active_nodes: 2, inference_backend: 'vllm' },
        { timestamp_ms: 3, throughput: 128, cpu: 42, memory: 58, gpu: 71, latency_p50: 32, latency_p95: 84, active_nodes: 2, inference_backend: 'vllm' },
      ],
    }),
    getBackends: vi.fn().mockResolvedValue({ available: [], current: 'cpu' }),
    getBackendStatus: vi.fn().mockResolvedValue({ status: undefined }),
    ...overrides,
  };
}

describe('MetricsTab', () => {
  beforeEach(() => {
    useAppStore.setState({
      metrics: {
        throughput: 128.5,
        cpu: 42,
        memory: 58,
        gpu: 71,
        latency_p50: 32,
        latency_p95: 84,
        active_nodes: 2,
        total_vram_gb: 16,
        total_memory_gb: 32,
        used_memory_gb: 18,
        gpu_available: true,
        real_inference: true,
        samples: 5,
        uptime_s: 600,
        inference_backend: 'vllm',
      },
      metricsHistory: [
        { t: 1, throughput: 100, cpu: 30, memory: 50, gpu: 60, latency_p50: 40, latency_p95: 90 },
        { t: 2, throughput: 120, cpu: 35, memory: 52, gpu: 62, latency_p50: 38, latency_p95: 86 },
        { t: 3, throughput: 128, cpu: 42, memory: 58, gpu: 71, latency_p50: 32, latency_p95: 84 },
      ],
      workers: [],
    });
  });

  it('renders trend cards from metrics history', async () => {
    const api = createMockApi();
    render(<MetricsTab api={api} />);

    await waitFor(() => {
      expect(screen.getByText('Throughput Trend')).toBeInTheDocument();
      expect(screen.getByText('Latency p95 Trend')).toBeInTheDocument();
      expect(screen.getAllByText('3 recent samples').length).toBeGreaterThan(0);
      expect(screen.getByLabelText('Throughput Trend')).toBeInTheDocument();
      expect(screen.getByLabelText('Latency p95 Trend')).toBeInTheDocument();
    });
  });

  it('renders without a backend or workers', () => {
    const api = createMockApi();
    render(<MetricsTab api={api} />);
    expect(screen.getByText('System Performance')).toBeInTheDocument();
  });

  it('does not show the Cluster Load gauge with 0 or 1 workers', () => {
    useAppStore.setState({
      workers: [{ id: 'w1', host: '127.0.0.1', port: 8003, status: 'Connected', model: 'x', threads: 4, load: 50 }],
    });
    const api = createMockApi();
    render(<MetricsTab api={api} />);
    expect(screen.queryByText('Cluster Load')).not.toBeInTheDocument();
  });

  it('shows the Cluster Load gauge averaging worker load with 2+ workers', () => {
    useAppStore.setState({
      workers: [
        { id: 'w1', host: '127.0.0.1', port: 8003, status: 'Connected', model: 'x', threads: 4, load: 40 },
        { id: 'w2', host: '127.0.0.2', port: 8003, status: 'Connected', model: 'x', threads: 4, load: 60 },
      ],
    });
    const api = createMockApi();
    render(<MetricsTab api={api} />);
    expect(screen.getByText('Cluster Load')).toBeInTheDocument();
    expect(screen.getByText('50.0%')).toBeInTheDocument();
    expect(screen.getByText('avg across 2 workers')).toBeInTheDocument();
  });

  it('shows a real (not hardcoded) Backend Health panel once the status poll resolves', async () => {
    const api = createMockApi({
      getBackends: vi.fn().mockResolvedValue({ available: [], current: 'directml' }),
      getBackendStatus: vi.fn().mockResolvedValue({
        status: {
          backend: 'directml',
          device_name: 'AMD Radeon 860M',
          status: 'active',
          health: 'healthy',
          utilization: 42,
          temperature: 61,
        },
      }),
    });
    render(<MetricsTab api={api} />);

    await waitFor(() => expect(screen.getByText('Backend Health')).toBeInTheDocument());
    expect(screen.getByText('AMD Radeon 860M')).toBeInTheDocument();
    expect(screen.getByText('healthy')).toBeInTheDocument();
    expect(screen.getByText('42%')).toBeInTheDocument();
    expect(screen.getByText('61°C')).toBeInTheDocument();
  });

  it('renders "not monitored"/"not available" instead of fake numbers when the backend has no GPU reading', async () => {
    const api = createMockApi({
      getBackends: vi.fn().mockResolvedValue({ available: [], current: 'cpu' }),
      getBackendStatus: vi.fn().mockResolvedValue({
        status: { backend: 'cpu', device_name: 'CPU', status: 'active', health: 'healthy', utilization: null, temperature: null },
      }),
    });
    render(<MetricsTab api={api} />);

    await waitFor(() => expect(screen.getByText('Backend Health')).toBeInTheDocument());
    expect(screen.getByText('not monitored')).toBeInTheDocument();
    expect(screen.getByText('not available')).toBeInTheDocument();
  });

  it('disables CSV export with no history and enables it once samples exist', () => {
    const api = createMockApi();
    const { rerender } = render(<MetricsTab api={api} />);
    expect(screen.getByLabelText('Export metrics history as CSV')).not.toBeDisabled();

    useAppStore.setState({
      metricsHistory: [],
    });
    rerender(<MetricsTab api={api} />);
    expect(screen.getByLabelText('Export metrics history as CSV')).toBeDisabled();
  });

  it('triggers a toast notification upon CSV export', () => {
    const addToastSpy = vi.fn();
    useAppStore.setState({
      addToast: addToastSpy,
    });
    const api = createMockApi();
    render(<MetricsTab api={api} />);

    // Mock URL.createObjectURL and URL.revokeObjectURL for jsdom environment
    window.URL.createObjectURL = vi.fn().mockReturnValue('blob:mock-url');
    window.URL.revokeObjectURL = vi.fn();

    const exportBtn = screen.getByLabelText('Export metrics history as CSV');
    fireEvent.click(exportBtn);

    expect(addToastSpy).toHaveBeenCalledWith({
      type: 'success',
      message: 'Exported 3 metrics samples to CSV.',
    });
  });
});
