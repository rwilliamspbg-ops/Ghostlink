import React, { useState, useEffect, useMemo } from 'react';
import { RefreshCw, Server, Shield, Cpu, Activity, Power, Plus, HeartPulse } from 'lucide-react';
import { ClusterTopology } from '../api';
import { useAppStore } from '../store';
import { EmptyState } from './StatusViews';

// Badge used to be hardcoded green regardless of this value — a
// disconnected/error worker still showed the healthy-green pulse.
// `healthyCount` above already branches on 'connected' correctly; this
// mirrors that same check for the per-card badge.
function workerStatusClasses(status: string | undefined): { pill: string; dot: string } {
  if (status?.toLowerCase() === 'connected') {
    return { pill: 'bg-green-500/10 text-green-400', dot: 'bg-green-400 animate-pulse' };
  }
  return { pill: 'bg-red-500/10 text-red-400', dot: 'bg-red-400' };
}

export const WorkersTab: React.FC<{ api: any }> = ({ api }) => {
  const { workers, setWorkers, addToast } = useAppStore();
  const [loading, setLoading] = useState(false);
  const [topology, setTopology] = useState<ClusterTopology | null>(null);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [historyWindow, setHistoryWindow] = useState<8 | 16 | 32>(16);
  const healthyCount = useMemo(
    () => workers.filter((w) => w.status?.toLowerCase() === 'connected').length,
    [workers]
  );
  const [showAddForm, setShowAddForm] = useState(false);
  const [addHost, setAddHost] = useState('');
  const [addPort, setAddPort] = useState('8003');
  const [addError, setAddError] = useState('');

  const topologyLayout = useMemo(() => {
    if (!topology?.nodes?.length) return [];
    const centerX = 170;
    const centerY = 110;
    const radius = Math.max(40, 90 - topology.nodes.length * 2);
    return topology.nodes.map((node, index) => {
      if (index === 0) {
        return { ...node, x: centerX, y: centerY, root: true };
      }
      const angle = ((index - 1) / Math.max(1, topology.nodes.length - 1)) * Math.PI * 2 - Math.PI / 2;
      return {
        ...node,
        x: centerX + Math.cos(angle) * radius,
        y: centerY + Math.sin(angle) * radius,
        root: false,
      };
    });
  }, [topology]);

  const selectedNode = useMemo(() => {
    if (!topology?.nodes?.length) return null;
    return topology.nodes.find((node) => node.id === selectedNodeId) || topology.nodes[0];
  }, [selectedNodeId, topology]);

  const buildSparkline = (values: number[], width: number, height: number) => {
    if (!values.length) return '';
    const min = Math.min(...values);
    const max = Math.max(...values);
    const range = max - min || 1;
    return values
      .map((value, index) => {
        const x = values.length === 1 ? width / 2 : (index / (values.length - 1)) * width;
        const y = height - ((value - min) / range) * (height - 6) - 3;
        return `${x},${y}`;
      })
      .join(' ');
  };

  const refreshWorkers = async () => {
    setLoading(true);
    const [result, topologyResult] = await Promise.all([
      api.getWorkers(),
      api.getClusterTopology ? api.getClusterTopology() : Promise.resolve({ topology: null }),
    ]);
    if (!result.error) {
      setWorkers(result.workers);
    }
    if (!topologyResult.error && topologyResult.topology) {
      setTopology(topologyResult.topology);
      setSelectedNodeId((current) => {
        if (current && topologyResult.topology.nodes.some((node: any) => node.id === current)) {
          return current;
        }
        return topologyResult.topology.nodes[0]?.id || null;
      });
    }
    setLoading(false);
  };

  // CRITICAL FIX #2: Auto-refresh polling every 5 seconds
  useEffect(() => {
    refreshWorkers();
    
    const interval = setInterval(() => {
      refreshWorkers();
    }, 5000);

    return () => clearInterval(interval);
  }, [api, setWorkers]);

  // CRITICAL FIX #3: Add disconnect handler
  const handleDisconnectWorker = async (workerId: string, workerHost: string) => {
    if (!window.confirm(`Are you sure you want to disconnect worker ${workerHost}? This will remove it from the active cluster pool.`)) {
      return;
    }
    const result = await api.disconnectWorker(workerId);
    if (result.success) {
      refreshWorkers();
    } else {
      addToast({ type: 'error', message: result.error || `Failed to disconnect ${workerHost}` });
    }
  };

  const handleAddWorker = async () => {
    setAddError('');
    if (!addHost.trim()) {
      setAddError('Host is required');
      return;
    }
    const port = parseInt(addPort, 10);
    if (isNaN(port) || port < 1 || port > 65535) {
      setAddError('Port must be 1-65535');
      return;
    }
    const result = await api.addWorker(addHost.trim(), port);
    if (result.success) {
      setShowAddForm(false);
      setAddHost('');
      setAddPort('8003');
      refreshWorkers();
    } else {
      setAddError(result.error || 'Failed to add worker');
    }
  };

  return (
    <div className="flex flex-col h-full bg-slate-950">
      <div className="flex items-center justify-between px-6 py-4 border-b border-slate-900 sticky top-0 bg-slate-950/50 backdrop-blur-md z-10">
        <div className="flex items-center gap-3">
            <h2 className="text-xl font-bold text-white">Cluster Workers</h2>
            <div className="px-2 py-0.5 bg-blue-600/20 text-blue-400 rounded text-[10px] font-bold uppercase tracking-wider">
                {workers.length} Nodes
            </div>
            {workers.length > 0 && (
                <div
                    className={`flex items-center gap-1.5 px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider ${
                        healthyCount === workers.length
                            ? 'bg-green-500/10 text-green-400'
                            : healthyCount === 0
                            ? 'bg-red-500/10 text-red-400'
                            : 'bg-amber-500/10 text-amber-400'
                    }`}
                    title="Nodes reporting a Connected status"
                >
                    <HeartPulse size={11} aria-hidden="true" />
                    {healthyCount}/{workers.length} healthy
                </div>
            )}
        </div>
        <div className="flex items-center gap-2">
            <button
                onClick={() => { setShowAddForm(!showAddForm); setAddError(''); }}
                aria-haspopup="true"
                aria-expanded={showAddForm}
                className="flex items-center gap-2 px-3 py-1.5 bg-blue-600 hover:bg-blue-500 text-white text-xs font-bold rounded-lg transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
            >
                <Plus size={14} aria-hidden="true" /> Add Worker
            </button>
            <button
                onClick={async () => {
                  const r = await api.discoverPeers();
                  if (r.success) {
                    refreshWorkers();
                  } else {
                    addToast({ type: 'error', message: r.error || 'Failed to discover peers' });
                  }
                }}
                className="flex items-center gap-2 px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-white text-xs font-bold rounded-lg transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                title="Discover peers on network"
            >
                <RefreshCw size={14} aria-hidden="true" /> Discover
            </button>
            <button
                onClick={refreshWorkers}
                aria-label="Refresh workers"
                className="p-2 rounded-lg hover:bg-slate-900 text-slate-400 hover:text-white transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
            >
                <RefreshCw size={18} className={loading ? 'animate-spin' : ''} aria-hidden="true" />
            </button>
        </div>
      </div>

      {showAddForm && (
        // eslint-disable-next-line jsx-a11y/no-static-element-interactions -- wrapper only delegates Escape from the real interactive inputs/buttons inside, not a control itself
        <div
          className="px-6 py-4 border-b border-slate-800 bg-slate-900/30"
          onKeyDown={(e) => { if (e.key === 'Escape') setShowAddForm(false); }}
        >
          <div className="flex items-end gap-3 max-w-lg">
            <div className="flex-1">
              <label htmlFor="host" className="block text-[10px] font-bold uppercase tracking-wider text-slate-400 mb-1">Host</label>
              <input
                type="text"
                value={addHost}
                onChange={(e) => setAddHost(e.target.value)}
                placeholder="192.168.1.100"
                name="host"
                id="host"
                className="w-full px-3 py-1.5 bg-slate-800 border border-slate-700 rounded-lg text-sm text-slate-200 placeholder-slate-500 focus:outline-none focus:border-blue-500 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
              />
            </div>
            <div className="w-24">
              <label htmlFor="port" className="block text-[10px] font-bold uppercase tracking-wider text-slate-400 mb-1">Port</label>
              <input
                type="number"
                value={addPort}
                onChange={(e) => setAddPort(e.target.value)}
                placeholder="8003"
                name="port"
                id="port"
                className="w-full px-3 py-1.5 bg-slate-800 border border-slate-700 rounded-lg text-sm text-slate-200 placeholder-slate-500 focus:outline-none focus:border-blue-500 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
              />
            </div>
            <button
              onClick={handleAddWorker}
              className="px-4 py-1.5 bg-green-600 hover:bg-green-500 text-white text-xs font-bold rounded-lg transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
            >
              Connect
            </button>
          </div>
          {addError && <p role="alert" className="text-red-400 text-xs mt-2">{addError}</p>}
        </div>
      )}

      <div className="flex-1 overflow-y-auto p-6" tabIndex={0} role="region" aria-label="Workers">
        <div className="max-w-5xl mx-auto">
            {topology && (
              <div className="mb-6 bg-slate-900/50 border border-slate-800 rounded-3xl p-6 overflow-hidden">
                <div className="flex items-start justify-between gap-4 mb-6">
                  <div>
                    <h3 className="text-lg font-bold text-white">Logical Topology</h3>
                    <p className="text-xs text-slate-500 mt-1">
                      {topology.summary.active_nodes}/{topology.summary.node_count} active nodes · {topology.summary.total_vram_gb.toFixed(1)} GB cluster VRAM · {topology.summary.total_system_memory_gb.toFixed(1)} GB system memory
                    </p>
                  </div>
                  <div className="text-right text-xs text-slate-500">
                    <div>{topology.edges.length} interconnects</div>
                    <div>{topology.nodes.length > 0 ? topology.nodes[0].label : 'No root'} root node</div>
                  </div>
                </div>

                <div className="grid grid-cols-1 lg:grid-cols-[360px,1fr] gap-6">
                  <div className="rounded-3xl bg-slate-950/80 border border-slate-800 p-4">
                    <svg viewBox="0 0 340 220" className="w-full h-[220px]" role="img" aria-label="Cluster topology graph">
                      {topologyLayout.slice(1).map((node) => (
                        <line
                          key={`edge-${node.id}`}
                          x1={170}
                          y1={110}
                          x2={node.x}
                          y2={node.y}
                          stroke="rgba(59,130,246,0.35)"
                          strokeWidth="2"
                        />
                      ))}
                      {topologyLayout.map((node) => (
                        <g key={node.id} transform={`translate(${node.x}, ${node.y})`}>
                          <circle
                            r={node.root ? 26 : 20}
                            className={selectedNode?.id === node.id ? 'fill-cyan-500/20 stroke-cyan-400' : node.root ? 'fill-blue-500/20 stroke-blue-400' : 'fill-slate-900 stroke-slate-500'}
                            strokeWidth="2"
                          />
                          <text x="0" y="4" textAnchor="middle" className="fill-white text-[10px] font-bold">
                            {node.label.slice(0, 8)}
                          </text>
                        </g>
                      ))}
                    </svg>
                  </div>

                  <div className="space-y-4">
                    <div className="flex flex-wrap gap-2">
                      {topology.nodes.map((node) => (
                        <button
                          key={node.id}
                          onClick={() => setSelectedNodeId(node.id)}
                          className={`px-3 py-2 rounded-xl text-xs font-bold transition border focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none ${selectedNode?.id === node.id ? 'bg-cyan-500/10 text-cyan-300 border-cyan-500/30' : 'bg-slate-950/60 text-slate-400 border-slate-800 hover:text-white hover:border-slate-700'}`}
                        >
                          {node.label}
                        </button>
                      ))}
                    </div>

                    <div className="flex items-center justify-between gap-3">
                      <div className="text-xs text-slate-500">
                        Focus node detail and recent history window
                      </div>
                      <div className="flex items-center gap-2">
                        {[8, 16, 32].map((size) => (
                          <button
                            key={size}
                            onClick={() => setHistoryWindow(size as 8 | 16 | 32)}
                            className={`px-2.5 py-1 rounded-lg text-[10px] font-bold transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none ${historyWindow === size ? 'bg-blue-600 text-white' : 'bg-slate-900 text-slate-400 hover:text-white'}`}
                          >
                            {size}
                          </button>
                        ))}
                      </div>
                    </div>

                    {selectedNode && (
                      <div className="rounded-2xl border border-slate-800 bg-slate-950/60 px-4 py-4 flex items-start justify-between gap-4">
                        <div className="min-w-0 flex-1">
                          <div className="text-sm font-bold text-slate-100">{selectedNode.label}</div>
                          <div className="text-[10px] text-slate-500 font-mono">
                            {selectedNode.compute_capability} · {selectedNode.vram_gb.toFixed(1)} GB VRAM · {selectedNode.system_memory_gb.toFixed(1)} GB RAM
                            {selectedNode.ip_address ? ` · ${selectedNode.ip_address}` : ''}
                          </div>
                          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 mt-3">
                            <MiniTrend
                              label="Latency"
                              value={selectedNode.latency_us > 0 ? `${selectedNode.latency_us.toFixed(0)} us` : 'No latency yet'}
                              colorClass="text-orange-400"
                              points={buildSparkline((selectedNode.latency_history_us || []).slice(-historyWindow), 120, 28)}
                            />
                            <MiniTrend
                              label="Throughput"
                              value={selectedNode.throughput_gbps > 0 ? `${selectedNode.throughput_gbps.toFixed(2)} GB/s` : 'No throughput yet'}
                              colorClass="text-cyan-400"
                              points={buildSparkline((selectedNode.throughput_history_gbps || []).slice(-historyWindow), 120, 28)}
                            />
                          </div>
                        </div>
                        <div className="text-right text-xs text-slate-400">
                          <div>{selectedNode.status}</div>
                          <div>{selectedNode.latency_us > 0 ? `${selectedNode.latency_us.toFixed(0)} us` : 'No latency yet'}</div>
                          <div>{selectedNode.throughput_gbps > 0 ? `${selectedNode.throughput_gbps.toFixed(2)} GB/s` : 'No throughput yet'}</div>
                        </div>
                      </div>
                    )}
                  </div>
                </div>
              </div>
            )}
            {workers.length === 0 ? (
              <EmptyState
                icon={Server}
                title="No workers connected"
                description="Add a worker by host/port, or discover peers on the local network."
              />
            ) : (
              <div className="grid grid-cols-1 gap-4">
                {workers.map((worker) => (
                  <div key={worker.id} className="bg-slate-900/50 border border-slate-800 rounded-3xl p-6 hover:border-slate-700 transition-all relative overflow-hidden group">
                    <div className="absolute top-0 right-0 p-4">
                        <div className={`flex items-center gap-2 px-3 py-1 rounded-full text-[10px] font-bold ${workerStatusClasses(worker.status).pill}`}>
                            <div className={`w-1.5 h-1.5 rounded-full ${workerStatusClasses(worker.status).dot}`}></div>
                            {worker.status}
                        </div>
                    </div>

                    <div className="flex items-start gap-5">
                      <div className="p-4 bg-slate-800 rounded-2xl text-slate-400 group-hover:bg-blue-600 group-hover:text-white transition-colors">
                        <Server size={28} />
                      </div>

                      <div className="flex-1">
                          <div className="flex flex-col mb-6">
                              <h3 className="text-lg font-bold text-slate-100">{worker.host}</h3>
                              <p className="text-xs text-slate-500 font-mono">ID: {worker.id}</p>
                          </div>

                          <div className="grid grid-cols-2 md:grid-cols-4 gap-6">
                              <div className="space-y-1">
                                  <div className="flex items-center gap-1.5 text-slate-500">
                                      <Cpu size={12} />
                                      <span className="text-[10px] font-bold uppercase tracking-wider">Resources</span>
                                  </div>
                                  <p className="text-sm font-bold text-slate-200">{worker.threads} Threads</p>
                              </div>
                              <div className="space-y-1">
                                  <div className="flex items-center gap-1.5 text-slate-500">
                                      <Activity size={12} />
                                      <span className="text-[10px] font-bold uppercase tracking-wider">Current Load</span>
                                  </div>
                                  <div className="flex items-center gap-2">
                                      <div className="flex-1 h-1.5 bg-slate-800 rounded-full overflow-hidden">
                                          <div
                                              className="h-full bg-blue-500 transition-all duration-500"
                                              style={{ width: `${worker.load}%` }}
                                          ></div>
                                      </div>
                                      <span className="text-sm font-bold text-slate-200">{worker.load}%</span>
                                  </div>
                              </div>
                              <div className="space-y-1">
                                  <div className="flex items-center gap-1.5 text-slate-500">
                                      <Shield size={12} />
                                      <span className="text-[10px] font-bold uppercase tracking-wider">Model</span>
                                  </div>
                                  <p className="text-sm font-bold text-slate-200 truncate">{worker.model}</p>
                              </div>
                              <div className="flex items-end justify-end">
                                  <button
                                    onClick={() => handleDisconnectWorker(worker.id, worker.host)}
                                    className="p-2 text-slate-500 hover:text-red-400 hover:bg-red-500/10 rounded-lg transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                                    title="Disconnect worker"
                                    aria-label={`Disconnect worker ${worker.host}`}
                                  >
                                      <Power size={20} aria-hidden="true" />
                                  </button>
                              </div>
                          </div>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            )}
        </div>
      </div>
    </div>
  );
};

const MiniTrend = ({
  label,
  value,
  colorClass,
  points,
}: {
  label: string;
  value: string;
  colorClass: string;
  points: string;
}) => (
  <div className="rounded-xl border border-slate-800 bg-slate-900/70 px-3 py-2">
    <div className="flex items-center justify-between gap-2 mb-1">
      <span className="text-[10px] uppercase tracking-wider text-slate-500 font-bold">{label}</span>
      <span className={`text-[10px] font-bold ${colorClass}`}>{value}</span>
    </div>
    <svg viewBox="0 0 120 28" className="w-full h-7" role="img" aria-label={`${label} trend`}>
      {points ? (
        <polyline
          fill="none"
          stroke="currentColor"
          strokeWidth="2.5"
          strokeLinecap="round"
          strokeLinejoin="round"
          points={points}
          className={colorClass}
        />
      ) : (
        <text x="60" y="18" textAnchor="middle" className="fill-slate-600 text-[8px]">
          Waiting
        </text>
      )}
    </svg>
  </div>
);
