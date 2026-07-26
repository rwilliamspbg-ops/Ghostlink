import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { MetricsTab } from './MetricsTab';
import { useAppStore } from '../store';

function createMockApi(overrides: Partial<Record<string, any>> = {}) {
  return {
    getMetrics: vi.fn().mockResolvedValue({ metrics: null }),
    getBackends: vi.fn().mockResolvedValue({ available: [], current: 'cpu' }),
    getBackendStatus: vi.fn().mockResolvedValue({ status: undefined }),
    ...overrides,
  };
}

describe('MetricsTab', () => {
  beforeEach(() => {
    useAppStore.setState({
      metrics: null,
      metricsHistory: [],
      workers: [],
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
    expect(screen.getByLabelText('Export metrics history as CSV')).toBeDisabled();

    useAppStore.setState({
      metricsHistory: [{ t: Date.now(), throughput: 10, cpu: 5, memory: 5, gpu: 0, latency_p50: 1, latency_p95: 2 }],
    });
    rerender(<MetricsTab api={api} />);
    expect(screen.getByLabelText('Export metrics history as CSV')).not.toBeDisabled();
  });
});
