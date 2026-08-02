import { describe, it, expect, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { MetricsTab } from './MetricsTab';
import { useAppStore } from '../store';

function createMockApi() {
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
  };
}

describe('MetricsTab', () => {
  it('renders trend cards from metrics history', async () => {
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
      setMetrics: vi.fn(),
    } as any);

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
});