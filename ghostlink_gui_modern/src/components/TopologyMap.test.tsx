import { describe, it, expect } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { TopologyMap } from './TopologyMap';
import { ClusterTopology } from '../api';

const mockTopology: ClusterTopology = {
  summary: {
    node_count: 3,
    active_nodes: 2,
    total_vram_gb: 48,
    total_system_memory_gb: 80,
  },
  nodes: [
    {
      id: 'node-a',
      label: 'Main Server',
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
      label: 'GPU Worker 1',
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
    {
      id: 'node-c',
      label: 'Old Laptop',
      compute_capability: 'cpu',
      vram_gb: 0,
      system_memory_gb: 32,
      status: 'Inactive',
      latency_us: 0,
      throughput_gbps: 0,
      latency_history_us: [],
      throughput_history_gbps: [],
      ip_address: '192.168.1.30',
      rpc_port: null,
      contribute_compute: false,
      build_id_status: 'mismatch',
      secret_status: 'n/a',
      allowlist_status: 'n/a',
      role: 'unused',
      excluded_reason: 'Build ID mismatch',
    },
  ],
  edges: [
    { from: 'node-a', to: 'node-b', latency_us: 180, throughput_gbps: 2.4 },
    { from: 'node-a', to: 'node-c', latency_us: 0, throughput_gbps: 0 },
  ],
};

const emptyTopology: ClusterTopology = {
  summary: { node_count: 0, active_nodes: 0, total_vram_gb: 0, total_system_memory_gb: 0 },
  nodes: [],
  edges: [],
};

describe('TopologyMap', () => {
  it('renders the SVG topology map', () => {
    render(<TopologyMap topology={mockTopology} />);
    expect(screen.getByRole('img', { name: /cluster topology map/i })).toBeInTheDocument();
  });

  it('renders all nodes with labels', () => {
    render(<TopologyMap topology={mockTopology} />);
    expect(screen.getByText('Main Server')).toBeInTheDocument();
    expect(screen.getByText('GPU Worker 1')).toBeInTheDocument();
    expect(screen.getByText('Old Laptop')).toBeInTheDocument();
  });

  it('renders node circles for each node', () => {
    render(<TopologyMap topology={mockTopology} />);
    const nodeButtons = screen.getAllByRole('button', { name: /node/i });
    expect(nodeButtons).toHaveLength(3);
  });

  it('shows empty state when no nodes exist', () => {
    render(<TopologyMap topology={emptyTopology} />);
    expect(screen.getByText(/no topology data available/i)).toBeInTheDocument();
  });

  it('shows detail popover when a node is clicked', () => {
    render(<TopologyMap topology={mockTopology} />);
    const nodeButton = screen.getByRole('button', { name: /gpu worker 1/i });
    fireEvent.click(nodeButton);

    // Popover heading shows the node name (SVG label also matches)
    expect(screen.getAllByText('GPU Worker 1').length).toBeGreaterThan(0);
    expect(screen.getByText('192.168.1.20:50052')).toBeInTheDocument();
    expect(screen.getByText('24 GB')).toBeInTheDocument();
    expect(screen.getByText('180 µs')).toBeInTheDocument();
  });

  it('hides detail popover when close button is clicked', () => {
    render(<TopologyMap topology={mockTopology} />);
    const nodeButton = screen.getByRole('button', { name: /main server/i });
    fireEvent.click(nodeButton);

    expect(screen.getByText('127.0.0.1')).toBeInTheDocument();

    const closeButton = screen.getByRole('button', { name: /close details/i });
    fireEvent.click(closeButton);

    expect(screen.queryByText('127.0.0.1')).not.toBeInTheDocument();
  });

  it('renders edge latency labels', () => {
    render(<TopologyMap topology={mockTopology} />);
    expect(screen.getByText('180µs')).toBeInTheDocument();
  });

  it('renders legend items', () => {
    render(<TopologyMap topology={mockTopology} />);
    expect(screen.getByText('Coordinator')).toBeInTheDocument();
    expect(screen.getByText('Contributor')).toBeInTheDocument();
    expect(screen.getByText('Excluded/Unused')).toBeInTheDocument();
  });

  it('shows excluded reason for unused nodes', () => {
    render(<TopologyMap topology={mockTopology} />);
    const nodeButton = screen.getByRole('button', { name: /old laptop/i });
    fireEvent.click(nodeButton);

    expect(screen.getByText('Build ID mismatch')).toBeInTheDocument();
  });

  it('applies custom className', () => {
    const { container } = render(
      <TopologyMap topology={mockTopology} className="custom-class" />,
    );
    expect(container.firstChild).toHaveClass('custom-class');
  });
});
