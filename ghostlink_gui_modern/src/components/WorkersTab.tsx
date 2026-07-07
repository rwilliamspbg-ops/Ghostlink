import React, { useState, useEffect, useCallback, useMemo } from 'react';
import { RefreshCw, Plus, Plug, Radio, X, Wifi } from 'lucide-react';
import { useAppStore, Worker } from '../store';

interface WorkersTabProps {
  api: any;
}

export const WorkersTab: React.FC<WorkersTabProps> = React.memo(({ api }) => {
  const { workers, setWorkers } = useAppStore();
  const [loading, setLoading] = useState(false);
  const [host, setHost] = useState('127.0.0.1');
  const [port, setPort] = useState('8004');
  const [message, setMessage] = useState('');
  const [discovering, setDiscovering] = useState(false);

  const refreshWorkers = useCallback(async () => {
    setLoading(true);
    const result = await api.getWorkers();
    if (!result.error) {
      setWorkers(result.workers);
    }
    setLoading(false);
  }, [api, setWorkers]);

  useEffect(() => {
    refreshWorkers();
    const interval = setInterval(refreshWorkers, 2000);
    return () => clearInterval(interval);
  }, [refreshWorkers]);

  const handleAddWorker = useCallback(async () => {
    if (!host || !port) {
      setMessage('Enter host and port');
      return;
    }

    const result = await api.addWorker(host, parseInt(port));
    if (result.success) {
      setMessage(`Added worker: ${host}:${port}`);
      setHost('127.0.0.1');
      setPort('8004');
      setTimeout(() => refreshWorkers(), 500);
      setTimeout(() => setMessage(''), 3000);
    } else {
      setMessage(`Error: ${result.error}`);
    }
  }, [host, port, api, refreshWorkers]);

  const handleDiscoverPeers = useCallback(async () => {
    setDiscovering(true);
    const result = await api.discoverPeers?.();
    if (result?.success) {
      setMessage(`Found ${result.count || 0} peers`);
      setTimeout(() => refreshWorkers(), 500);
    }
    setDiscovering(false);
  }, [api, refreshWorkers]);

  const handleConnectWorkers = useCallback(async () => {
    const result = await api.connectWorkers();
    if (result.success) {
      setMessage('Workers connected');
      setTimeout(() => refreshWorkers(), 500);
      setTimeout(() => setMessage(''), 3000);
    } else {
      setMessage(`Error: ${result.error}`);
    }
  }, [api, refreshWorkers]);

  const handleDisconnectWorker = useCallback(async (workerId: string) => {
    const result = await api.disconnectWorker?.(workerId);
    if (result?.success) {
      setMessage('Worker disconnected');
      setTimeout(() => refreshWorkers(), 500);
    } else {
      setMessage(`Error: ${result?.error || 'Failed to disconnect'}`);
    }
  }, [api, refreshWorkers]);

  const onlineCount = useMemo(() => workers.filter((w) => w.status?.toLowerCase() === 'online' || w.status === 'Connected').length, [workers]);
  const averageLoad = useMemo(() => {
    if (workers.length === 0) return 0;
    return workers.reduce((sum, w) => sum + (w.load || 0), 0) / workers.length;
  }, [workers]);

  return (
    <div className="space-y-4">
      {/* Status Summary */}
      <div className="grid grid-cols-3 gap-3">
        <div className="bg-slate-900 border border-slate-700 rounded p-4">
          <p className="text-slate-400 text-xs mb-1">Total Workers</p>
          <p className="text-2xl font-bold text-slate-100">{workers.length}</p>
        </div>
        <div className="bg-slate-900 border border-slate-700 rounded p-4">
          <p className="text-slate-400 text-xs mb-1">Online</p>
          <p className="text-2xl font-bold text-emerald-400">{onlineCount}</p>
        </div>
        <div className="bg-slate-900 border border-slate-700 rounded p-4">
          <p className="text-slate-400 text-xs mb-1">Avg Cluster Load</p>
          <p className="text-2xl font-bold text-blue-400">{averageLoad.toFixed(1)}%</p>
        </div>
      </div>

      {/* Add Worker */}
      <div className="bg-slate-900 rounded p-4 border border-slate-700 space-y-3">
        <h3 className="text-sm font-semibold text-slate-200">Add Worker</h3>
        <div className="grid grid-cols-3 gap-2">
          <input
            type="text"
            placeholder="Host (e.g., 192.168.1.100)"
            value={host}
            onChange={(e) => setHost(e.target.value)}
            className="px-3 py-2 bg-slate-800 border border-slate-700 rounded text-slate-100 placeholder-slate-500 focus:outline-none focus:border-blue-500 text-sm"
          />
          <input
            type="number"
            placeholder="Port"
            value={port}
            onChange={(e) => setPort(e.target.value)}
            className="px-3 py-2 bg-slate-800 border border-slate-700 rounded text-slate-100 placeholder-slate-500 focus:outline-none focus:border-blue-500 text-sm"
          />
          <button
            onClick={handleAddWorker}
            className="flex items-center justify-center gap-2 bg-green-600 hover:bg-green-700 text-white rounded transition text-sm"
          >
            <Plus size={16} />
            Add
          </button>
        </div>
      </div>

      {/* Controls */}
      <div className="flex gap-2 flex-wrap">
        <button
          onClick={refreshWorkers}
          disabled={loading}
          className="flex items-center gap-2 px-4 py-2 bg-slate-700 hover:bg-slate-600 disabled:bg-slate-800 text-white rounded transition text-sm"
        >
          <RefreshCw size={16} className={loading ? 'animate-spin' : ''} />
          Refresh
        </button>
        <button
          onClick={handleConnectWorkers}
          className="flex items-center gap-2 px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded transition text-sm"
        >
          <Plug size={16} />
          Connect Network
        </button>
        <button
          onClick={handleDiscoverPeers}
          disabled={discovering}
          className="flex items-center gap-2 px-4 py-2 bg-purple-600 hover:bg-purple-700 disabled:bg-slate-800 text-white rounded transition text-sm"
        >
          <Radio size={16} className={discovering ? 'animate-spin' : ''} />
          Discover Peers
        </button>
      </div>

      {message && <div className="p-3 rounded bg-blue-900 text-blue-200 text-sm">{message}</div>}

      {/* Workers Table */}
      <div className="overflow-x-auto rounded border border-slate-700">
        <table className="w-full">
          <thead className="bg-slate-900 border-b border-slate-700">
            <tr>
              <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">ID</th>
              <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Host</th>
              <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Port</th>
              <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Status</th>
              <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Model</th>
              <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Threads</th>
              <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Load</th>
              <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Health</th>
              <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Action</th>
            </tr>
          </thead>
          <tbody>
            {workers.length === 0 ? (
              <tr>
                <td colSpan={9} className="px-4 py-4 text-center text-slate-500">
                  No workers connected. Add or discover workers to get started.
                </td>
              </tr>
            ) : (
              workers.map((worker) => {
                const isOnline = worker.status?.toLowerCase() === 'online' || worker.status === 'Connected';
                const loadPercent = worker.load || 0;

                return (
                  <tr key={worker.id} className="border-b border-slate-700 hover:bg-slate-800">
                    <td className="px-4 py-3 text-sm text-slate-200 font-mono">{worker.id.slice(0, 8)}...</td>
                    <td className="px-4 py-3 text-sm text-slate-400">{worker.host}</td>
                    <td className="px-4 py-3 text-sm text-slate-400">{worker.port}</td>
                    <td className="px-4 py-3 text-sm">
                      <div className="flex items-center gap-2">
                        <div
                          className={`w-2.5 h-2.5 rounded-full ${
                            isOnline ? 'bg-emerald-400' : 'bg-red-400'
                          }`}
                        />
                        <span className={isOnline ? 'text-emerald-400' : 'text-red-400'}>
                          {worker.status}
                        </span>
                      </div>
                    </td>
                    <td className="px-4 py-3 text-sm text-slate-200">{worker.model || '-'}</td>
                    <td className="px-4 py-3 text-sm text-slate-400">{worker.threads}</td>
                    <td className="px-4 py-3 text-sm">
                      <div className="flex items-center gap-2">
                        <div className="w-12 h-2 bg-slate-700 rounded overflow-hidden">
                          <div
                            className={`h-full ${
                              loadPercent < 50
                                ? 'bg-green-500'
                                : loadPercent < 80
                                ? 'bg-yellow-500'
                                : 'bg-red-500'
                            }`}
                            style={{ width: `${loadPercent}%` }}
                          />
                        </div>
                        <span className="text-xs text-slate-400 w-8">{loadPercent.toFixed(0)}%</span>
                      </div>
                    </td>
                    <td className="px-4 py-3 text-sm">
                      <div className="flex items-center gap-1">
                        <Wifi
                          size={14}
                          className={isOnline ? 'text-emerald-400' : 'text-slate-600'}
                        />
                        <span className="text-xs text-slate-400">
                          {isOnline ? 'Good' : 'Poor'}
                        </span>
                      </div>
                    </td>
                    <td className="px-4 py-3 text-sm">
                      <button
                        onClick={() => handleDisconnectWorker(worker.id)}
                        className="text-red-400 hover:text-red-300 transition"
                        title="Disconnect"
                      >
                        <X size={16} />
                      </button>
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
};
