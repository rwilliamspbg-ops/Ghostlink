import { describe, it, expect, vi } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { WorkersTab } from './WorkersTab';
import { useAppStore } from '../store';

function createMockApi() {
  return {
    getWorkers: vi.fn().mockResolvedValue({
      workers: [
        { id: 'node-a', host: '127.0.0.1', port: 8003, status: 'Connected', model: 'tinyllama', threads: 8, load: 22 },
        { id: 'node-b', host: '192.168.1.20', port: 8003, status: 'Connected', model: 'llama3.2:3b', threads: 12, load: 47 },
      ],
    }),
    getClusterTopology: vi.fn().mockResolvedValue({
      topology: {
        summary: {
          node_count: 2,
          active_nodes: 2,
          total_vram_gb: 24,
          total_system_memory_gb: 48,
        },
        nodes: [
          { id: 'node-a', label: 'node-a', compute_capability: 'cpu', vram_gb: 0, system_memory_gb: 16, status: 'Active', latency_us: 0, throughput_gbps: 0, latency_history_us: [], throughput_history_gbps: [], ip_address: '127.0.0.1' },
          { id: 'node-b', label: 'node-b', compute_capability: 'cuda', vram_gb: 24, system_memory_gb: 32, status: 'Active', latency_us: 180, throughput_gbps: 2.4, latency_history_us: [140, 160, 180], throughput_history_gbps: [1.8, 2.1, 2.4], ip_address: '192.168.1.20' },
        ],
        edges: [
          { from: 'node-a', to: 'node-b', latency_us: 180, throughput_gbps: 2.4 },
        ],
      },
    }),
    disconnectWorker: vi.fn().mockResolvedValue({ success: true }),
    addWorker: vi.fn().mockResolvedValue({ success: true }),
    discoverPeers: vi.fn().mockResolvedValue({ success: true, count: 1 }),
  };
}

describe('WorkersTab', () => {
  it('renders topology and worker inventory', async () => {
    useAppStore.setState({
      workers: [
        { id: 'node-a', host: '127.0.0.1', port: 8003, status: 'Connected', model: 'tinyllama', threads: 8, load: 22 },
        { id: 'node-b', host: '192.168.1.20', port: 8003, status: 'Connected', model: 'llama3.2:3b', threads: 12, load: 47 },
      ],
      setWorkers: vi.fn(),
    } as any);

    const api = createMockApi();
    render(<WorkersTab api={api} />);

    await waitFor(() => {
      expect(screen.getByText('Logical Topology')).toBeInTheDocument();
      expect(screen.getByText(/2\/2 active nodes/i)).toBeInTheDocument();
      expect(screen.getByLabelText('Cluster topology graph')).toBeInTheDocument();
      expect(screen.getAllByLabelText('Latency trend').length).toBeGreaterThan(0);
      expect(screen.getAllByLabelText('Throughput trend').length).toBeGreaterThan(0);
      expect(screen.getAllByText('node-a').length).toBeGreaterThan(0);
      expect(screen.getAllByText('node-b').length).toBeGreaterThan(0);
      expect(screen.getByText('127.0.0.1')).toBeInTheDocument();
      expect(screen.getByText('192.168.1.20')).toBeInTheDocument();
      expect(screen.getAllByText('Connected').length).toBeGreaterThan(0);
    });

    fireEvent.click(screen.getByRole('button', { name: 'node-b' }));
    fireEvent.click(screen.getByRole('button', { name: '8' }));

    await waitFor(() => {
      expect(screen.getByText(/Focus node detail and recent history window/i)).toBeInTheDocument();
      expect(screen.getAllByText('2.40 GB/s').length).toBeGreaterThan(0);
    });
  });
});