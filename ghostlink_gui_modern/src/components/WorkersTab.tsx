import React, { useState, useEffect, useMemo, useCallback } from 'react';
import {
  RefreshCw,
  Server,
  Activity,
  Power,
  Plus,
  Network,
  ChevronDown,
  ChevronUp,
  AlertTriangle,
  HelpCircle,
  Layers,
} from 'lucide-react';
import { ClusterTopology, ClusterTopologyNode } from '../api';
import { useAppStore, Settings } from '../store';
import { EmptyState } from './StatusViews';

export const WorkersTab: React.FC<{ api: any }> = ({ api }) => {
  const { setWorkers, addToast } = useAppStore();
  const [loading, setLoading] = useState(false);
  const [topology, setTopology] = useState<ClusterTopology | null>(null);

  // Settings state for controls
  const [settings, setSettings] = useState<Settings | null>(null);
  const [showAdvanced, setShowAddvanced] = useState(false);
  const [showHowToJoin, setShowHowToJoin] = useState(false);
  const [showAddForm, setShowAddForm] = useState(false);
  const [showContributeWarning, setShowContributeWarning] = useState(false);

  // Form states
  const [addHost, setAddHost] = useState('');
  const [addPort, setAddPort] = useState('8003');
  const [addError, setAddError] = useState('');

  // Advanced settings editable local fields
  const [editRpcPort, setEditRpcPort] = useState<number>(50052);
  const [editSharedSecret, setEditSharedSecret] = useState<string>('');
  const [editAllowlist, setEditAllowlist] = useState<string>('');

  const fetchSettings = useCallback(async () => {
    const res = await api.getSettings();
    if (res && !res.error && res.settings) {
      setSettings(res.settings);
      setEditRpcPort(res.settings.rpc_port ?? 50052);
      setEditAllowlist((res.settings.rpc_allowed_peers || []).join(', '));
    }
  }, [api]);

  const refreshWorkersAndTopology = useCallback(async () => {
    setLoading(true);
    const [workersRes, topologyRes] = await Promise.all([
      api.getWorkers(),
      api.getClusterTopology ? api.getClusterTopology() : Promise.resolve({ topology: null }),
    ]);

    if (!workersRes.error && workersRes.workers) {
      setWorkers(workersRes.workers);
    }

    if (!topologyRes.error && topologyRes.topology) {
      setTopology(topologyRes.topology);
    }
    setLoading(false);
  }, [api, setWorkers]);

  useEffect(() => {
    fetchSettings();
    refreshWorkersAndTopology();

    const interval = setInterval(() => {
      refreshWorkersAndTopology();
    }, 5000);

    return () => clearInterval(interval);
  }, [fetchSettings, refreshWorkersAndTopology]);

  // Handle escape key for modals
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setShowAddForm(false);
        setShowHowToJoin(false);
        setShowContributeWarning(false);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  const handleToggleDistributed = async (enabled: boolean) => {
    const res = await api.updateSettings({ distributed_inference: enabled });
    if (res.success) {
      addToast({
        type: 'success',
        message: enabled
          ? 'Distributed inference enabled. Ghostlink will split large models across LAN peers.'
          : 'Distributed inference disabled. Using single-machine inference.',
      });
      fetchSettings();
      refreshWorkersAndTopology();
    } else {
      addToast({
        type: 'error',
        message: res.error || 'Failed to update distributed inference setting.',
      });
    }
  };

  const handleToggleContribute = async (enabled: boolean) => {
    if (enabled) {
      setShowContributeWarning(true);
      return;
    }
    const res = await api.updateSettings({ contribute_compute: false });
    if (res.success) {
      addToast({
        type: 'info',
        message: 'Contribute compute disabled for this machine.',
      });
      fetchSettings();
      refreshWorkersAndTopology();
    } else {
      addToast({ type: 'error', message: res.error || 'Failed to disable contribute compute.' });
    }
  };

  const confirmEnableContribute = async () => {
    setShowContributeWarning(false);
    const res = await api.updateSettings({ contribute_compute: true });
    if (res.success) {
      addToast({
        type: 'success',
        message: 'Contribute compute enabled. LAN peers can now offload compute to this machine.',
      });
      fetchSettings();
      refreshWorkersAndTopology();
    } else {
      addToast({ type: 'error', message: res.error || 'Failed to enable contribute compute.' });
    }
  };

  const handleSaveAdvanced = async () => {
    const allowlistArr = editAllowlist
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean);

    const updatePayload: Partial<Settings> = {
      rpc_port: Number(editRpcPort),
      rpc_allowed_peers: allowlistArr,
    };

    if (editSharedSecret.trim().length > 0) {
      updatePayload.rpc_shared_secret = editSharedSecret.trim();
    }

    const res = await api.updateSettings(updatePayload);
    if (res.success) {
      addToast({ type: 'success', message: 'RPC settings saved successfully.' });
      setEditSharedSecret('');
      fetchSettings();
      refreshWorkersAndTopology();
    } else {
      addToast({ type: 'error', message: res.error || 'Failed to save RPC settings.' });
    }
  };

  const handleClearSharedSecret = async () => {
    const res = await api.updateSettings({ rpc_shared_secret: '' });
    if (res.success) {
      addToast({ type: 'info', message: 'Shared secret cleared.' });
      setEditSharedSecret('');
      fetchSettings();
      refreshWorkersAndTopology();
    } else {
      addToast({ type: 'error', message: res.error || 'Failed to clear shared secret.' });
    }
  };

  const handleDiscoverPeers = async () => {
    setLoading(true);
    const res = await api.discoverWorkers ? await api.discoverWorkers() : { count: 0 };
    if (res.error) {
      addToast({ type: 'error', message: res.error });
    } else {
      addToast({
        type: 'success',
        message: `Peer discovery complete. Found ${res.discovered ?? res.count ?? 0} active peers on LAN.`,
      });
      refreshWorkersAndTopology();
    }
    setLoading(false);
  };

  const handleAddWorkerSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setAddError('');
    const port = parseInt(addPort, 10);
    if (!addHost.trim()) {
      setAddError('Host address is required.');
      return;
    }
    if (isNaN(port) || port < 1 || port > 65535) {
      setAddError('Please enter a valid port number (1-65535).');
      return;
    }

    const res = await api.addWorker(addHost.trim(), port);
    if (res.success) {
      addToast({ type: 'success', message: `Added worker ${addHost.trim()}:${port}` });
      setShowAddForm(false);
      setAddHost('');
      setAddPort('8003');
      refreshWorkersAndTopology();
    } else {
      setAddError(res.error || 'Failed to add worker.');
    }
  };

  const handleDisconnectWorker = async (nodeId: string, hostLabel: string) => {
    if (
      !window.confirm(
        `Are you sure you want to disconnect ${hostLabel}? This will remove it from the active cluster pool.`
      )
    ) {
      return;
    }
    const res = await api.disconnectWorker ? await api.disconnectWorker(nodeId) : { success: true };
    if (res.success) {
      addToast({ type: 'success', message: `Disconnected worker ${hostLabel} successfully.` });
      refreshWorkersAndTopology();
    } else {
      addToast({ type: 'error', message: res.error || 'Failed to disconnect worker.' });
    }
  };

  const peers = useMemo(() => {
    if (!topology?.nodes) return [];
    return topology.nodes.slice(1); // 0 is this machine
  }, [topology]);

  const thisMachineNode = useMemo(() => {
    return topology?.nodes?.[0] || null;
  }, [topology]);

  return (
    <div className="p-8 max-w-7xl mx-auto space-y-8">
      {/* Header */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold text-slate-100 flex items-center gap-3">
            <Network className="text-blue-400" size={28} />
            Cluster Topology
          </h1>
          <p className="text-slate-400 text-sm mt-1">
            Zero-config distributed LAN inference cluster map and peer status overview.
          </p>
        </div>

        <div className="flex items-center gap-3">
          <button
            onClick={handleDiscoverPeers}
            disabled={loading}
            className="flex items-center gap-2 px-4 py-2 bg-slate-800 hover:bg-slate-700 text-slate-200 rounded-xl text-xs font-bold transition border border-slate-700 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
          >
            <Network size={16} />
            Discover Peers
          </button>

          <button
            onClick={() => setShowAddForm(true)}
            className="flex items-center gap-2 px-4 py-2 bg-slate-800 hover:bg-slate-700 text-slate-200 rounded-xl text-xs font-bold transition border border-slate-700 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
          >
            <Plus size={16} />
            Add Worker
          </button>

          <button
            onClick={refreshWorkersAndTopology}
            disabled={loading}
            className="flex items-center gap-2 px-3 py-2 bg-slate-800 hover:bg-slate-700 text-slate-200 rounded-xl text-xs font-bold transition border border-slate-700 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
            title="Refresh Cluster State"
            aria-label="Refresh Cluster State"
          >
            <RefreshCw size={16} className={loading ? 'animate-spin' : ''} />
          </button>
        </div>
      </div>

      {/* Primary Control & Placement Plan Banner */}
      <div className="bg-slate-900/80 border border-slate-800 rounded-3xl p-6 space-y-6 shadow-xl">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-6 border-b border-slate-800/80">
          <div className="space-y-1">
            <div className="text-sm font-bold text-slate-100 flex items-center gap-2">
              <Layers className="text-blue-400" size={18} />
              <span>Use other machines when this model does not fit</span>
            </div>
            <p className="text-xs text-slate-400">
              Automatically shards inference across available LAN peers via ggml-rpc when enabled.
            </p>
          </div>

          <label className="relative inline-flex items-center cursor-pointer select-none" aria-label="Use other machines toggle">
            <input
              type="checkbox"
              checked={!!settings?.distributed_inference}
              onChange={(e) => handleToggleDistributed(e.target.checked)}
              className="sr-only peer"
            />
            <div className="w-11 h-6 bg-slate-800 peer-focus:outline-none peer-focus:ring-2 peer-focus:ring-blue-500 rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-slate-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-blue-600"></div>
            <span className="ml-3 text-xs font-bold text-slate-300">
              {settings?.distributed_inference ? 'Distributed On' : 'Single Machine'}
            </span>
          </label>
        </div>

        {/* Placement Plan Details */}
        <div className="bg-slate-950/60 rounded-2xl border border-slate-800/80 p-5 space-y-3">
          <div className="flex items-center justify-between text-xs font-bold text-slate-300">
            <span className="flex items-center gap-2 uppercase tracking-wider text-slate-400 text-[10px]">
              <Activity size={14} className="text-cyan-400" />
              Placement Plan
            </span>
            <span
              className={`px-2.5 py-0.5 rounded-full text-[10px] ${
                topology?.placement_plan?.distributed_active
                  ? 'bg-blue-500/10 text-blue-400 border border-blue-500/30'
                  : 'bg-slate-800 text-slate-400'
              }`}
            >
              {topology?.placement_plan?.distributed_active ? 'Multi-Node RPC Split' : 'Local Only'}
            </span>
          </div>

          <p className="text-sm font-semibold text-slate-200 leading-relaxed font-mono">
            {topology?.placement_plan?.summary_text || 'Load a model to see a split'}
          </p>

          {topology?.placement_plan?.has_plan && topology.placement_plan.tensor_splits.length > 0 && (
            <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-3 pt-2">
              {topology.placement_plan.tensor_splits.map((split, i) => (
                <div key={i} className="p-3 bg-slate-900/90 rounded-xl border border-slate-800 text-xs space-y-1">
                  <div className="flex justify-between font-bold text-slate-200">
                    <span className="truncate">{split.label}</span>
                    <span className="text-cyan-400">{(split.percentage * 100).toFixed(0)}%</span>
                  </div>
                  <div className="w-full bg-slate-800 h-1.5 rounded-full overflow-hidden">
                    <div
                      className="bg-cyan-500 h-full transition-all duration-300"
                      style={{ width: `${(split.percentage * 100).toFixed(0)}%` }}
                    />
                  </div>
                  <div className="text-[10px] text-slate-500">
                    Weight {split.weight.toFixed(2)} · {split.vram_gb.toFixed(1)} GB VRAM
                  </div>
                </div>
              ))}
            </div>
          )}

          {topology?.placement_plan?.rpc_hosts && topology.placement_plan.rpc_hosts.length > 0 && (
            <div className="text-[10px] text-slate-500 font-mono pt-1">
              Active RPC targets (--rpc): {topology.placement_plan.rpc_hosts.join(', ')}
            </div>
          )}
        </div>

        {/* Advanced Settings Drawer */}
        <div className="border-t border-slate-800/80 pt-4">
          <button
            onClick={() => setShowAddvanced(!showAdvanced)}
            className="flex items-center gap-2 text-xs font-bold text-slate-400 hover:text-slate-200 transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none rounded-lg px-2 py-1"
          >
            {showAdvanced ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
            <span>Advanced Contributor & Security Settings</span>
          </button>

          {showAdvanced && (
            <div className="mt-4 p-5 bg-slate-950/80 border border-slate-800 rounded-2xl space-y-5">
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-4 border-b border-slate-800">
                <div>
                  <h4 className="text-xs font-bold text-slate-200 uppercase tracking-wider">
                    Contribute local GPU/CPU compute
                  </h4>
                  <p className="text-[11px] text-slate-400">
                    Allows other machines on the LAN to shard inference layers onto this node.
                  </p>
                </div>
                <label className="relative inline-flex items-center cursor-pointer select-none" aria-label="Contribute compute toggle">
                  <input
                    type="checkbox"
                    checked={!!settings?.contribute_compute}
                    onChange={(e) => handleToggleContribute(e.target.checked)}
                    className="sr-only peer"
                  />
                  <div className="w-11 h-6 bg-slate-800 peer-focus:outline-none peer-focus:ring-2 peer-focus:ring-blue-500 rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-slate-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-green-600"></div>
                </label>
              </div>

              <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                <div className="space-y-1">
                  <label htmlFor="edit-rpc-port" className="text-xs font-bold text-slate-300">RPC Advertised Port</label>
                  <input
                    id="edit-rpc-port"
                    type="number"
                    value={editRpcPort}
                    onChange={(e) => setEditRpcPort(parseInt(e.target.value, 10) || 50052)}
                    className="w-full bg-slate-900 border border-slate-800 rounded-xl px-3 py-2 text-xs text-slate-200 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                  />
                </div>

                <div className="space-y-1">
                  <label htmlFor="edit-shared-secret" className="text-xs font-bold text-slate-300 flex items-center justify-between">
                    <span>Shared Secret</span>
                    {settings?.rpc_shared_secret ? (
                      <span className="text-[10px] text-green-400 font-normal">Configured</span>
                    ) : (
                      <span className="text-[10px] text-slate-500 font-normal">Unset</span>
                    )}
                  </label>
                  <input
                    id="edit-shared-secret"
                    type="password"
                    placeholder={settings?.rpc_shared_secret ? '•••••••• (Write-only)' : 'Enter shared secret'}
                    value={editSharedSecret}
                    onChange={(e) => setEditSharedSecret(e.target.value)}
                    className="w-full bg-slate-900 border border-slate-800 rounded-xl px-3 py-2 text-xs text-slate-200 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                  />
                </div>

                <div className="space-y-1">
                  <label htmlFor="edit-allowlist" className="text-xs font-bold text-slate-300">Allowed Peers (IP / CIDR)</label>
                  <input
                    id="edit-allowlist"
                    type="text"
                    placeholder="e.g. 192.168.1.50, 10.0.0.0/24"
                    value={editAllowlist}
                    onChange={(e) => setEditAllowlist(e.target.value)}
                    className="w-full bg-slate-900 border border-slate-800 rounded-xl px-3 py-2 text-xs text-slate-200 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                  />
                </div>
              </div>

              <div className="flex items-center justify-between pt-2">
                {settings?.rpc_shared_secret ? (
                  <button
                    onClick={handleClearSharedSecret}
                    className="text-xs text-red-400 hover:text-red-300 transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none rounded px-2 py-1"
                  >
                    Clear Shared Secret
                  </button>
                ) : (
                  <div />
                )}

                <button
                  onClick={handleSaveAdvanced}
                  className="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-xl text-xs font-bold transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                >
                  Save RPC Settings
                </button>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Cluster Nodes Map */}
      <div className="space-y-4">
        <h2 className="text-lg font-bold text-slate-100 flex items-center justify-between">
          <span>Cluster Nodes</span>
          <span className="text-xs text-slate-400 font-normal">
            {topology?.summary?.node_count || 1} Total Nodes ({topology?.summary?.active_nodes || 1} Active)
          </span>
        </h2>

        {/* This Machine Card */}
        {thisMachineNode && (
          <div className="bg-slate-900/60 border-2 border-blue-500/40 rounded-3xl p-6 space-y-4 relative overflow-hidden shadow-lg">
            <div className="absolute top-0 right-0 p-4">
              <span className="px-3 py-1 bg-blue-500/20 text-blue-300 border border-blue-500/30 rounded-full text-[10px] font-bold uppercase tracking-wider">
                Coordinator (This Machine)
              </span>
            </div>

            <div className="flex items-start gap-4">
              <div className="p-3 bg-blue-600/20 text-blue-400 rounded-2xl border border-blue-500/30">
                <Server size={28} />
              </div>
              <div>
                <h3 className="text-base font-bold text-slate-100">{thisMachineNode.id}</h3>
                <p className="text-xs text-slate-400 font-mono">
                  {thisMachineNode.label} · {thisMachineNode.compute_capability}
                </p>
              </div>
            </div>

            <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-xs bg-slate-950/40 p-4 rounded-2xl border border-slate-800/60">
              <div>
                <span className="text-slate-500 text-[10px] uppercase font-bold">VRAM</span>
                <p className="font-bold text-slate-200">{thisMachineNode.vram_gb.toFixed(1)} GB</p>
              </div>
              <div>
                <span className="text-slate-500 text-[10px] uppercase font-bold">System RAM</span>
                <p className="font-bold text-slate-200">{thisMachineNode.system_memory_gb.toFixed(1)} GB</p>
              </div>
              <div>
                <span className="text-slate-500 text-[10px] uppercase font-bold">RPC Advertised</span>
                <p className="font-bold text-slate-200">
                  {thisMachineNode.rpc_port ? `Port ${thisMachineNode.rpc_port}` : 'Not contributing'}
                </p>
              </div>
              <div>
                <span className="text-slate-500 text-[10px] uppercase font-bold">Status</span>
                <p className="font-bold text-green-400">{thisMachineNode.status}</p>
              </div>
            </div>
          </div>
        )}

        {/* Discovered Peer Cards or Empty State */}
        {peers.length === 0 ? (
          <EmptyState
            icon={Server}
            title="No other machines yet"
            description="Discover peers on your local network or join another machine to share VRAM."
            action={{
              label: 'How to join',
              icon: HelpCircle,
              onClick: () => setShowHowToJoin(true),
            }}
          />
        ) : (
          <div className="grid grid-cols-1 gap-4">
            {peers.map((node: ClusterTopologyNode) => {
              const isUsed = node.role === 'contributor';
              const isExcluded = node.role === 'unused';

              return (
                <div
                  key={node.id}
                  className={`bg-slate-900/50 border rounded-3xl p-6 transition-all relative space-y-4 ${
                    isUsed
                      ? 'border-green-500/40 hover:border-green-500/60'
                      : 'border-slate-800 hover:border-slate-700'
                  }`}
                >
                  <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
                    <div className="flex items-center gap-3">
                      <div className="p-3 bg-slate-800 rounded-2xl text-slate-400">
                        <Server size={24} />
                      </div>
                      <div>
                        <h3 className="text-base font-bold text-slate-100">{node.id}</h3>
                        <p className="text-xs text-slate-500 font-mono">
                          {node.label} · {node.ip_address || 'No IP'}
                        </p>
                      </div>
                    </div>

                    <div className="flex items-center gap-2">
                      <span
                        className={`px-3 py-1 rounded-full text-[10px] font-bold ${
                          isUsed
                            ? 'bg-green-500/10 text-green-400 border border-green-500/30'
                            : 'bg-amber-500/10 text-amber-400 border border-amber-500/30'
                        }`}
                      >
                        {isUsed ? 'Active Contributor' : 'Unused / Excluded'}
                      </span>

                      <button
                        onClick={() => handleDisconnectWorker(node.id, node.label)}
                        className="p-2 text-slate-500 hover:text-red-400 hover:bg-red-500/10 rounded-lg transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                        title="Disconnect worker"
                        aria-label={`Disconnect worker ${node.label}`}
                      >
                        <Power size={18} aria-hidden="true" />
                      </button>
                    </div>
                  </div>

                  <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-xs bg-slate-950/50 p-4 rounded-2xl border border-slate-800/80">
                    <div>
                      <span className="text-slate-500 text-[10px] uppercase font-bold">VRAM / RAM</span>
                      <p className="font-bold text-slate-200">
                        {node.vram_gb.toFixed(1)} GB VRAM · {node.system_memory_gb.toFixed(1)} GB RAM
                      </p>
                    </div>
                    <div>
                      <span className="text-slate-500 text-[10px] uppercase font-bold">RPC Port</span>
                      <p className="font-bold text-slate-200">
                        {node.rpc_port ? `Port ${node.rpc_port}` : 'Not contributing'}
                      </p>
                    </div>
                    <div>
                      <span className="text-slate-500 text-[10px] uppercase font-bold">Build ID</span>
                      <p
                        className={`font-bold ${
                          node.build_id_status === 'match'
                            ? 'text-green-400'
                            : node.build_id_status === 'mismatch'
                            ? 'text-red-400'
                            : 'text-slate-400'
                        }`}
                      >
                        {node.build_id_status}
                      </p>
                    </div>
                    <div>
                      <span className="text-slate-500 text-[10px] uppercase font-bold">Secret / Allowlist</span>
                      <p className="font-bold text-slate-200">
                        Secret: {node.secret_status} · Allow: {node.allowlist_status}
                      </p>
                    </div>
                  </div>

                  {/* Exclusion Reason Banner */}
                  {isExcluded && node.excluded_reason && (
                    <div className="p-3 bg-amber-500/10 border border-amber-500/30 rounded-2xl flex items-center gap-3 text-xs text-amber-300">
                      <AlertTriangle size={16} className="text-amber-400 shrink-0" />
                      <div>
                        <span className="font-bold">Peer not used: </span>
                        {node.excluded_reason}
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* How to Join Modal */}
      {showHowToJoin && (
        <div className="fixed inset-0 bg-slate-950/80 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-slate-900 border border-slate-800 rounded-3xl p-6 max-w-lg w-full space-y-6 shadow-2xl">
            <div className="flex items-center justify-between pb-4 border-b border-slate-800">
              <h3 className="text-base font-bold text-slate-100 flex items-center gap-2">
                <Network className="text-blue-400" size={20} />
                How to join a second machine
              </h3>
              <button
                onClick={() => setShowHowToJoin(false)}
                className="text-slate-400 hover:text-white p-1 rounded-lg focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
              >
                ✕
              </button>
            </div>

            <div className="space-y-4 text-xs text-slate-300">
              <div className="flex items-start gap-3">
                <div className="w-6 h-6 rounded-full bg-blue-600/20 text-blue-400 border border-blue-500/30 flex items-center justify-center font-bold text-xs shrink-0">
                  1
                </div>
                <p>Install Ghostlink on the second machine on the same LAN.</p>
              </div>

              <div className="flex items-start gap-3">
                <div className="w-6 h-6 rounded-full bg-blue-600/20 text-blue-400 border border-blue-500/30 flex items-center justify-center font-bold text-xs shrink-0">
                  2
                </div>
                <p>
                  In Workers tab &gt; Advanced, enable <strong>Contribute this machine's GPU/CPU</strong>.
                </p>
              </div>

              <div className="flex items-start gap-3">
                <div className="w-6 h-6 rounded-full bg-blue-600/20 text-blue-400 border border-blue-500/30 flex items-center justify-center font-bold text-xs shrink-0">
                  3
                </div>
                <p>Ensure both machines use the same llama.cpp build version and same shared secret (if set).</p>
              </div>
            </div>

            <div className="pt-2 flex justify-end">
              <button
                onClick={() => setShowHowToJoin(false)}
                className="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-xl text-xs font-bold transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
              >
                Got it
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Contribute Warning Modal */}
      {showContributeWarning && (
        <div className="fixed inset-0 bg-slate-950/80 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-slate-900 border border-slate-800 rounded-3xl p-6 max-w-md w-full space-y-5 shadow-2xl">
            <div className="flex items-center gap-3 text-amber-400">
              <AlertTriangle size={24} />
              <h3 className="text-base font-bold text-slate-100">LAN Security Warning</h3>
            </div>

            <p className="text-xs text-slate-300 leading-relaxed">
              Enabling compute contribution exposes an RPC server endpoint on your local network.
              <code>ggml-rpc-server</code> has no native protocol-level authentication. Only enable this on trusted LANs, or configure a Shared Secret and Allowed Peers list.
            </p>

            <div className="flex justify-end gap-3 pt-2">
              <button
                onClick={() => setShowContributeWarning(false)}
                className="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-xl text-xs font-bold transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
              >
                Cancel
              </button>
              <button
                onClick={confirmEnableContribute}
                className="px-4 py-2 bg-amber-600 hover:bg-amber-500 text-white rounded-xl text-xs font-bold transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
              >
                Enable Contribute
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Add Worker Modal Form */}
      {showAddForm && (
        <div className="fixed inset-0 bg-slate-950/80 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <form
            onSubmit={handleAddWorkerSubmit}
            className="bg-slate-900 border border-slate-800 rounded-3xl p-6 max-w-md w-full space-y-5 shadow-2xl"
          >
            <div className="flex items-center justify-between pb-3 border-b border-slate-800">
              <h3 className="text-base font-bold text-slate-100 flex items-center gap-2">
                <Plus className="text-blue-400" size={20} />
                Add Worker Manually
              </h3>
              <button
                type="button"
                onClick={() => setShowAddForm(false)}
                className="text-slate-400 hover:text-white p-1 rounded-lg focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
              >
                ✕
              </button>
            </div>

            {addError && <div className="text-xs text-red-400 bg-red-500/10 p-3 rounded-xl">{addError}</div>}

            <div className="space-y-4">
              <div className="space-y-1">
                <label htmlFor="add-host-input" className="text-xs font-bold text-slate-300">
                  Host Address <span className="text-red-400">*</span>
                </label>
                <input
                  id="add-host-input"
                  type="text"
                  required
                  placeholder="e.g. 192.168.1.100"
                  value={addHost}
                  onChange={(e) => setAddHost(e.target.value)}
                  className="w-full bg-slate-950 border border-slate-800 rounded-xl px-3 py-2 text-xs text-slate-200 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                />
              </div>

              <div className="space-y-1">
                <label htmlFor="add-port-input" className="text-xs font-bold text-slate-300">
                  Port <span className="text-red-400">*</span>
                </label>
                <input
                  id="add-port-input"
                  type="number"
                  required
                  placeholder="8003"
                  value={addPort}
                  onChange={(e) => setAddPort(e.target.value)}
                  className="w-full bg-slate-950 border border-slate-800 rounded-xl px-3 py-2 text-xs text-slate-200 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                />
              </div>
            </div>

            <div className="flex justify-end gap-3 pt-2">
              <button
                type="button"
                onClick={() => setShowAddForm(false)}
                className="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-xl text-xs font-bold transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
              >
                Cancel
              </button>
              <button
                type="submit"
                className="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-xl text-xs font-bold transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
              >
                Add Worker
              </button>
            </div>
          </form>
        </div>
      )}
    </div>
  );
};
