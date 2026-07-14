import React, { useState, useEffect } from 'react';
import { RefreshCw, Server, Shield, Cpu, Activity, Power, Plus } from 'lucide-react';
import { useAppStore } from '../store';

export const WorkersTab: React.FC<{ api: any }> = ({ api }) => {
  const { workers, setWorkers } = useAppStore();
  const [loading, setLoading] = useState(false);
  const [showAddForm, setShowAddForm] = useState(false);
  const [addHost, setAddHost] = useState('');
  const [addPort, setAddPort] = useState('8003');
  const [addError, setAddError] = useState('');

  const refreshWorkers = async () => {
    setLoading(true);
    const result = await api.getWorkers();
    if (!result.error) {
      setWorkers(result.workers);
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
  const handleDisconnectWorker = async (workerId: string) => {
    const result = await api.disconnectWorker(workerId);
    if (result.success) {
      refreshWorkers();
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
        </div>
        <div className="flex items-center gap-2">
            <button
                onClick={() => { setShowAddForm(!showAddForm); setAddError(''); }}
                className="flex items-center gap-2 px-3 py-1.5 bg-blue-600 hover:bg-blue-500 text-white text-xs font-bold rounded-lg transition"
            >
                <Plus size={14} /> Add Worker
            </button>
            <button
                onClick={refreshWorkers}
                className="p-2 rounded-lg hover:bg-slate-900 text-slate-400 hover:text-white transition"
            >
                <RefreshCw size={18} className={loading ? 'animate-spin' : ''} />
            </button>
        </div>
      </div>

      {showAddForm && (
        <div className="px-6 py-4 border-b border-slate-800 bg-slate-900/30">
          <div className="flex items-end gap-3 max-w-lg">
            <div className="flex-1">
              <label className="block text-[10px] font-bold uppercase tracking-wider text-slate-400 mb-1">Host</label>
              <input
                type="text"
                value={addHost}
                onChange={(e) => setAddHost(e.target.value)}
                placeholder="192.168.1.100"
                className="w-full px-3 py-1.5 bg-slate-800 border border-slate-700 rounded-lg text-sm text-slate-200 placeholder-slate-500 focus:outline-none focus:border-blue-500"
              />
            </div>
            <div className="w-24">
              <label className="block text-[10px] font-bold uppercase tracking-wider text-slate-400 mb-1">Port</label>
              <input
                type="number"
                value={addPort}
                onChange={(e) => setAddPort(e.target.value)}
                placeholder="8003"
                className="w-full px-3 py-1.5 bg-slate-800 border border-slate-700 rounded-lg text-sm text-slate-200 placeholder-slate-500 focus:outline-none focus:border-blue-500"
              />
            </div>
            <button
              onClick={handleAddWorker}
              className="px-4 py-1.5 bg-green-600 hover:bg-green-500 text-white text-xs font-bold rounded-lg transition"
            >
              Connect
            </button>
          </div>
          {addError && <p className="text-red-400 text-xs mt-2">{addError}</p>}
        </div>
      )}

      <div className="flex-1 overflow-y-auto p-6">
        <div className="max-w-5xl mx-auto">
            <div className="grid grid-cols-1 gap-4">
              {workers.map((worker) => (
                <div key={worker.id} className="bg-slate-900/50 border border-slate-800 rounded-3xl p-6 hover:border-slate-700 transition-all relative overflow-hidden group">
                  <div className="absolute top-0 right-0 p-4">
                      <div className="flex items-center gap-2 px-3 py-1 bg-green-500/10 text-green-400 rounded-full text-[10px] font-bold">
                          <div className="w-1.5 h-1.5 bg-green-400 rounded-full animate-pulse"></div>
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
                                  onClick={() => handleDisconnectWorker(worker.id)}
                                  className="p-2 text-slate-500 hover:text-red-400 hover:bg-red-500/10 rounded-lg transition"
                                  title="Disconnect worker"
                                >
                                    <Power size={20} />
                                </button>
                            </div>
                        </div>
                    </div>
                  </div>
                </div>
              ))}
            </div>
        </div>
      </div>
    </div>
  );
};
