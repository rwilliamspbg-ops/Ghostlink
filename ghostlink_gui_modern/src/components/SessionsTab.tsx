import React, { useState, useEffect } from 'react';
import { RefreshCw, XCircle, Clock, Database, Zap } from 'lucide-react';
import { useAppStore } from '../store';
import { EmptyState, ErrorPanel } from './StatusViews';

export const SessionsTab: React.FC<{ api: any }> = ({ api }) => {
  const { sessions, setSessions } = useAppStore();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const refreshSessions = async () => {
    setLoading(true);
    setError('');
    const result = await api.getSessions();
    if (!result.error) {
      setSessions(result.sessions);
    } else {
      setError(result.error);
    }
    setLoading(false);
  };

  useEffect(() => {
    refreshSessions();
  }, [api]);

  const handleCancel = async (id: string) => {
    await api.cancelSession(id);
    refreshSessions();
  };

  return (
    <div className="flex flex-col h-full bg-slate-950">
      <div className="flex items-center justify-between px-6 py-4 border-b border-slate-900 sticky top-0 bg-slate-950/50 backdrop-blur-md z-10">
        <h2 className="text-xl font-bold text-white">Active Sessions</h2>
        <button
          onClick={refreshSessions}
          aria-label="Refresh sessions"
          className="p-2 rounded-lg hover:bg-slate-900 text-slate-400 hover:text-white transition"
        >
          <RefreshCw size={18} className={loading ? 'animate-spin' : ''} aria-hidden="true" />
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        <div className="max-w-5xl mx-auto">
          {error ? (
            <ErrorPanel icon={XCircle} title="Connection Error" message={error} />
          ) : sessions.length === 0 ? (
            <EmptyState
              variant="card"
              icon={Clock}
              title="No active inference sessions"
              description="Start a chat to create a new session."
            />
          ) : (
            <div className="grid grid-cols-1 gap-4">
              {sessions.map((session) => (
                <div key={session.id} className="bg-slate-900/50 border border-slate-800 rounded-2xl p-5 hover:border-slate-700 transition-all group">
                  <div className="flex items-center justify-between mb-4">
                    <div className="flex items-center gap-4">
                      <div className="p-3 bg-blue-500/10 rounded-xl text-blue-400">
                        <Zap size={20} />
                      </div>
                      <div>
                        <h3 className="font-bold text-slate-200">{session.id}</h3>
                        <div className="flex items-center gap-2 text-xs text-slate-500">
                            <Database size={12} />
                            <span>{session.model}</span>
                        </div>
                      </div>
                    </div>
                    <div className="flex items-center gap-3">
                        <div className="px-3 py-1 rounded-full bg-green-500/10 text-green-400 text-[10px] font-bold uppercase tracking-wider">
                            {session.status}
                        </div>
                        <button
                            onClick={() => handleCancel(session.id)}
                            className="p-2 text-slate-500 hover:text-red-400 hover:bg-red-500/10 rounded-lg transition"
                            aria-label={`Cancel session ${session.id}`}
                        >
                            <XCircle size={18} aria-hidden="true" />
                        </button>
                    </div>
                  </div>

                  <div className="grid grid-cols-3 gap-4">
                    <div className="bg-slate-950/50 p-4 rounded-xl border border-slate-800/50">
                        <p className="text-[10px] text-slate-500 uppercase font-bold tracking-tighter mb-1">Throughput</p>
                        <p className="text-xl font-bold text-blue-400">{session.throughput} <span className="text-xs font-normal text-slate-600">t/s</span></p>
                    </div>
                    <div className="bg-slate-950/50 p-4 rounded-xl border border-slate-800/50">
                        <p className="text-[10px] text-slate-500 uppercase font-bold tracking-tighter mb-1">Latency</p>
                        <p className="text-xl font-bold text-orange-400">{session.latency} <span className="text-xs font-normal text-slate-600">ms</span></p>
                    </div>
                    <div className="bg-slate-950/50 p-4 rounded-xl border border-slate-800/50">
                        <p className="text-[10px] text-slate-500 uppercase font-bold tracking-tighter mb-1">Tokens</p>
                        <p className="text-xl font-bold text-purple-400">{session.tokens}</p>
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
