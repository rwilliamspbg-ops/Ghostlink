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
  }, [refreshWorkers]);

  const handleAddWorker = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoading(true);
    const result = await api.addWorker(host, parseInt(port));
    if (result.success) {
      setMessage('Worker added successfully');
      refreshWorkers();
    } else {
      setMessage(`Error: ${result.error}`);
    }
    setLoading(false);
  };

  const handleDiscover = async () => {
    setDiscovering(true);
    const result = await api.discoverPeers();
    if (result.success) {
      setMessage(`Discovery started. Found ${result.count} peers.`);
      setTimeout(refreshWorkers, 2000);
    } else {
      setMessage(`Discovery failed: ${result.error}`);
    }
    setDiscovering(false);
  };

  const handleDisconnect = async (id: string) => {
    const result = await api.disconnectWorker(id);
    if (result.success) {
      refreshWorkers();
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-xl font-bold text-white">Worker Orchestration</h2>
        <div className="flex gap-2">
          <button
            onClick={handleDiscover}
            disabled={discovering}
            className="flex items-center gap-2 px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-500 transition disabled:opacity-50"
          >
            <Radio size={18} className={discovering ? 'animate-pulse' : ''} />
            {discovering ? 'Discovering...' : 'Auto-Discover'}
          </button>
          <button
            onClick={refreshWorkers}
            className={`p-2 rounded bg-slate-800 text-slate-300 hover:bg-slate-700 transition ${
              loading ? 'animate-spin' : ''
            }`}
          >
            <RefreshCw size={20} />
          </button>
        </div>
      </div>

      {message && (
        <div className="bg-blue-900/30 border border-blue-500 text-blue-200 p-4 rounded-lg flex items-center justify-between">
          <span>{message}</span>
          <button onClick={() => setMessage('')}>
            <X size={18} />
          </button>
        </div>
      )}

      <div className="bg-slate-800/50 p-6 rounded-lg border border-slate-700">
        <h3 className="text-lg font-semibold text-white mb-4">Manual Node Addition</h3>
        <form onSubmit={handleAddWorker} className="flex flex-wrap gap-4 items-end">
          <div className="space-y-2">
            <label className="text-xs text-slate-400 uppercase font-bold">Node Host/IP</label>
            <input
              type="text"
              value={host}
              onChange={(e) => setHost(e.target.value)}
              className="bg-slate-900 border border-slate-700 text-white rounded px-4 py-2 focus:border-blue-500 outline-none"
              placeholder="127.0.0.1"
            />
          </div>
          <div className="space-y-2">
            <label className="text-xs text-slate-400 uppercase font-bold">Control Port</label>
            <input
              type="text"
              value={port}
              onChange={(e) => setPort(e.target.value)}
              className="bg-slate-900 border border-slate-700 text-white rounded px-4 py-2 focus:border-blue-500 outline-none w-24"
              placeholder="8004"
            />
          </div>
          <button
            type="submit"
            disabled={loading}
            className="flex items-center gap-2 px-6 py-2 bg-slate-700 text-white rounded hover:bg-slate-600 transition"
          >
            <Plus size={18} />
            Add Worker
          </button>
        </form>
      </div>

      <div className="overflow-x-auto">
        <table className="w-full text-left border-collapse">
          <thead>
            <tr className="border-b border-slate-700">
              <th className="py-4 px-4 text-xs font-bold text-slate-400 uppercase">Node ID / Host</th>
              <th className="py-4 px-4 text-xs font-bold text-slate-400 uppercase">Status</th>
              <th className="py-4 px-4 text-xs font-bold text-slate-400 uppercase">Active Model</th>
              <th className="py-4 px-4 text-xs font-bold text-slate-400 uppercase">Compute Load</th>
              <th className="py-4 px-4 text-xs font-bold text-slate-400 uppercase">Actions</th>
            </tr>
          </thead>
          <tbody>
            {workers.length === 0 ? (
              <tr>
                <td colSpan={5} className="py-8 text-center text-slate-500 italic">
                  No workers connected to the fabric.
                </td>
              </tr>
            ) : (
              workers.map((worker) => {
                return (
                  <tr key={worker.id} className="border-b border-slate-800 hover:bg-slate-800/30">
                    <td className="py-4 px-4">
                      <div className="flex items-center gap-3">
                        <div className="p-2 bg-slate-800 rounded">
                          <Plug className="text-blue-400" size={16} />
                        </div>
                        <div>
                          <p className="text-sm font-semibold text-white">{worker.id}</p>
                          <p className="text-xs text-slate-500">
                            {worker.host}:{worker.port}
                          </p>
                        </div>
                      </div>
                    </td>
                    <td className="py-4 px-4">
                      <span className="flex items-center gap-2 text-sm text-green-400">
                        <div className="w-2 h-2 rounded-full bg-green-400 animate-pulse" />
                        {worker.status}
                      </span>
                    </td>
                    <td className="py-4 px-4">
                      <span className="text-sm text-slate-300">{worker.model}</span>
                    </td>
                    <td className="py-4 px-4">
                      <div className="w-32">
                        <div className="flex items-center justify-between mb-1">
                          <span className="text-xs text-slate-400">{worker.load}%</span>
                        </div>
                        <div className="w-full bg-slate-800 rounded-full h-1.5">
                          <div
                            className="bg-blue-500 h-1.5 rounded-full"
                            style={{ width: `${worker.load}%` }}
                          />
                        </div>
                      </div>
                    </td>
                    <td className="py-4 px-4">
                      <button
                        onClick={() => handleDisconnect(worker.id)}
                        className="p-2 text-slate-500 hover:text-red-400 transition"
                        title="Disconnect Worker"
                      >
                        <Wifi size={18} />
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
});
