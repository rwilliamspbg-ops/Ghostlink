import React, { useState, useEffect, useCallback } from 'react';
import { RefreshCw, XCircle, Clock, Database, Zap } from 'lucide-react';
import { useAppStore } from '../store';
import { EmptyState, ErrorPanel } from './StatusViews';

// The status badge used to be hardcoded green regardless of this value, so a
// cancelled/error session rendered with the same "healthy" style as a saved
// one. Keys are matched case-insensitively against whatever the backend sent.
const SESSION_STATUS_CLASSES: Record<string, string> = {
  error: 'bg-red-500/10 text-red-400',
  failed: 'bg-red-500/10 text-red-400',
  cancelled: 'bg-red-500/10 text-red-400',
  downloading: 'bg-amber-500/10 text-amber-400',
  pending: 'bg-amber-500/10 text-amber-400',
};
function sessionStatusClasses(status: string): string {
  return SESSION_STATUS_CLASSES[status.toLowerCase()] ?? 'bg-green-500/10 text-green-400';
}

export const SessionsTab: React.FC<{ api: any }> = ({ api }) => {
  const { sessions, setSessions, addToast } = useAppStore();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const refreshSessions = useCallback(async () => {
    setLoading(true);
    setError('');
    const result = await api.getSessions();
    if (!result.error) {
      setSessions(result.sessions);
    } else {
      setError(result.error);
    }
    setLoading(false);
  }, [api, setSessions]);

  useEffect(() => {
    refreshSessions();
  }, [refreshSessions]);

  const handleCancel = async (id: string) => {
    const result = await api.cancelSession(id);
    if (result?.success === false) {
      addToast({ type: 'error', message: result.error || `Failed to cancel session ${id}` });
      return;
    }
    refreshSessions();
  };

  return (
    <div className="flex flex-col h-full bg-slate-950">
      <div className="flex items-center justify-between px-6 py-4 border-b border-slate-900 sticky top-0 bg-slate-950/50 backdrop-blur-md z-10">
        <h2 className="text-xl font-bold text-white">Active Sessions</h2>
        <button
          onClick={refreshSessions}
          aria-label="Refresh sessions"
          title="Refresh sessions"
          className="p-2 rounded-lg hover:bg-slate-900 text-slate-400 hover:text-white transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
        >
          <RefreshCw size={18} className={loading ? 'animate-spin' : ''} aria-hidden="true" />
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-6" tabIndex={0} role="region" aria-label="Sessions">
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
                        <div className={`px-3 py-1 rounded-full text-[10px] font-bold uppercase tracking-wider ${sessionStatusClasses(session.status)}`}>
                            {session.status}
                        </div>
                        <button
                            onClick={() => {
                              if (window.confirm(`Are you sure you want to cancel session ${session.id}? This will immediately terminate the running inference.`)) {
                                handleCancel(session.id);
                              }
                            }}
                            className="p-2 text-slate-500 hover:text-red-400 hover:bg-red-500/10 rounded-lg transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                            aria-label={`Cancel session ${session.id}`}
                            title={`Cancel session ${session.id}`}
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
