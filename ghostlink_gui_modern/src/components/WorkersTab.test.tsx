import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { WorkersTab } from './WorkersTab';
import { useAppStore } from '../store';

function createMockApi(overrides = {}) {
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
          {
            id: 'node-a',
            label: 'node-a',
            compute_capability: 'cpu',
            vram_gb: 0,
            system_memory_gb: 16,
            status: 'Active',
            latency_us: 0,
            throughput_gbps: 0,
            latency_history_us: [],
            throughput_history_gbps: [],
            ip_address: '127.0.0.1',
            rpc_port: null,
            contribute_compute: false,
            build_id_status: 'match',
            secret_status: 'n/a',
            allowlist_status: 'n/a',
            role: 'coordinator',
            excluded_reason: null,
          },
          {
            id: 'node-b',
            label: 'node-b',
            compute_capability: 'cuda',
            vram_gb: 24,
            system_memory_gb: 32,
            status: 'Active',
            latency_us: 180,
            throughput_gbps: 2.4,
            latency_history_us: [140, 160, 180],
            throughput_history_gbps: [1.8, 2.1, 2.4],
            ip_address: '192.168.1.20',
            rpc_port: 50052,
            contribute_compute: true,
            build_id_status: 'match',
            secret_status: 'n/a',
            allowlist_status: 'n/a',
            role: 'contributor',
            excluded_reason: null,
          },
        ],
        edges: [{ from: 'node-a', to: 'node-b', latency_us: 180, throughput_gbps: 2.4 }],
        placement_plan: {
          distributed_active: true,
          has_plan: true,
          model_name: 'llama3:30b',
          summary_text: 'Model llama3:30b with distributed on: split 0.60 on node-a (0 GB), 0.40 on node-b (24 GB).',
          tensor_splits: [
            { node_id: 'node-a', label: 'node-a', weight: 0.6, percentage: 0.6, vram_gb: 0 },
            { node_id: 'node-b', label: 'node-b', weight: 0.4, percentage: 0.4, vram_gb: 24 },
          ],
          rpc_hosts: ['192.168.1.20:50052'],
        },
      },
    }),
    getSettings: vi.fn().mockResolvedValue({
      settings: {
        distributed_inference: true,
        contribute_compute: false,
        rpc_port: 50052,
        rpc_allowed_peers: ['192.168.1.0/24'],
        rpc_shared_secret: '',
      },
    }),
    updateSettings: vi.fn().mockResolvedValue({ success: true }),
    disconnectWorker: vi.fn().mockResolvedValue({ success: true }),
    addWorker: vi.fn().mockResolvedValue({ success: true }),
    discoverWorkers: vi.fn().mockResolvedValue({ success: true, count: 1, discovered: 1 }),
    ...overrides,
  };
}

describe('WorkersTab', () => {
  beforeEach(() => {
    useAppStore.setState({
      workers: [
        { id: 'node-a', host: '127.0.0.1', port: 8003, status: 'Connected', model: 'tinyllama', threads: 8, load: 22 },
        { id: 'node-b', host: '192.168.1.20', port: 8003, status: 'Connected', model: 'llama3.2:3b', threads: 12, load: 47 },
      ],
      setWorkers: vi.fn(),
      addToast: vi.fn(),
    } as any);
  });

  it('renders coordinator card, placement plan, and contributor cards', async () => {
    const api = createMockApi();
    render(<WorkersTab api={api} />);

    await waitFor(() => {
      expect(screen.getByText('Cluster Topology')).toBeInTheDocument();
      expect(screen.getByText(/Use other machines when this model does not fit/i)).toBeInTheDocument();
      expect(screen.getByText(/Model llama3:30b with distributed on/i)).toBeInTheDocument();
      expect(screen.getByText('Coordinator (This Machine)')).toBeInTheDocument();
      expect(screen.getByText('Active Contributor')).toBeInTheDocument();
    });
  });

  it('renders peer exclusion reason when a peer is excluded', async () => {
    const api = createMockApi({
      getClusterTopology: vi.fn().mockResolvedValue({
        topology: {
          summary: { node_count: 2, active_nodes: 2, total_vram_gb: 24, total_system_memory_gb: 48 },
          nodes: [
            { id: 'node-a', label: 'node-a', compute_capability: 'cpu', vram_gb: 0, system_memory_gb: 16, status: 'Active' },
            {
              id: 'node-b',
              label: 'node-b',
              compute_capability: 'cuda',
              vram_gb: 24,
              system_memory_gb: 32,
              status: 'Active',
              rpc_port: 50052,
              build_id_status: 'mismatch',
              secret_status: 'n/a',
              allowlist_status: 'n/a',
              role: 'unused',
              excluded_reason: 'RPC build does not match coordinator',
            },
          ],
          edges: [],
          placement_plan: { distributed_active: false, has_plan: false, summary_text: 'Load a model to see a split', tensor_splits: [], rpc_hosts: [] },
        },
      }),
    });

    render(<WorkersTab api={api} />);

    await waitFor(() => {
      expect(screen.getByText('Unused / Excluded')).toBeInTheDocument();
      expect(screen.getByText('RPC build does not match coordinator')).toBeInTheDocument();
    });
  });

  it('toggles distributed inference and updates settings', async () => {
    const api = createMockApi();
    render(<WorkersTab api={api} />);

    await waitFor(() => {
      expect(screen.getByText('Distributed On')).toBeInTheDocument();
    });

    const toggle = screen.getByLabelText('Use other machines toggle');
    fireEvent.click(toggle);

    await waitFor(() => {
      expect(api.updateSettings).toHaveBeenCalledWith({ distributed_inference: false });
    });
  });

  it('shows LAN-trust confirmation warning when enabling contribute compute', async () => {
    const api = createMockApi();
    render(<WorkersTab api={api} />);

    await waitFor(() => {
      expect(screen.getByText('Advanced Contributor & Security Settings')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText('Advanced Contributor & Security Settings'));

    await waitFor(() => {
      expect(screen.getByText('Contribute local GPU/CPU compute')).toBeInTheDocument();
    });

    const contributeToggle = screen.getByLabelText('Contribute compute toggle');
    fireEvent.click(contributeToggle);

    await waitFor(() => {
      expect(screen.getByText('LAN Security Warning')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole('button', { name: 'Enable Contribute' }));

    await waitFor(() => {
      expect(api.updateSettings).toHaveBeenCalledWith({ contribute_compute: true });
    });
  });

  it('opens How To Join modal when empty state CTA is clicked', async () => {
    const api = createMockApi({
      getClusterTopology: vi.fn().mockResolvedValue({
        topology: {
          summary: { node_count: 1, active_nodes: 1, total_vram_gb: 16, total_system_memory_gb: 16 },
          nodes: [{ id: 'node-a', label: 'node-a', compute_capability: 'cpu', vram_gb: 0, system_memory_gb: 16, status: 'Active' }],
          edges: [],
        },
      }),
    });

    render(<WorkersTab api={api} />);

    await waitFor(() => {
      expect(screen.getByText('No other machines yet')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole('button', { name: /How to join/i }));

    await waitFor(() => {
      expect(screen.getByText('How to join a second machine')).toBeInTheDocument();
    });
  });
});
