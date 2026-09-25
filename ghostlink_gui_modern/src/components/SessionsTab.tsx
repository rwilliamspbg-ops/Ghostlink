import React, { useState, useEffect, useCallback } from 'react';
import { RefreshCw, XCircle, Clock, Database, Zap, MessageSquare, FolderOpen, Trash2, Loader } from 'lucide-react';
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
  const { sessions, setSessions, addToast, setActiveTab, createThread, renameThread, setCurrentModel } = useAppStore();
  const [loading, setLoading] = useState(false);
  const [actionSessionId, setActionSessionId] = useState<string | null>(null);
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
    setActionSessionId(id);
    try {
      const result = await api.cancelSession(id);
      if (result?.success === false) {
        addToast({ type: 'error', message: result.error || `Failed to cancel session ${id}` });
        return;
      }
      addToast({ type: 'info', message: `Cancelled session ${id}` });
      await refreshSessions();
    } finally {
      setActionSessionId(null);
    }
  };

  const handleOpen = async (id: string) => {
    setActionSessionId(id);
    try {
      const result = await api.loadSession(id);
      if (!result.success || !result.session) {
        addToast({ type: 'error', message: result.error || `Failed to open session ${id}` });
        return;
      }

      const messages = Array.isArray(result.session.messages)
        ? result.session.messages
            .filter((message: any) => message?.role === 'user' || message?.role === 'assistant')
            .map((message: any, index: number) => ({
              role: message.role,
              content: String(message.content ?? ''),
              id: `${result.session.id}-${index}`,
              timestamp: '',
            }))
        : [];
      const thread = createThread(messages, result.session.model);
      renameThread(thread.id, result.session.name || result.session.id);
      if (result.session.model) setCurrentModel(result.session.model);
      addToast({ type: 'success', message: `Opened session ${result.session.name || id}` });
      setActiveTab(0);
    } finally {
      setActionSessionId(null);
    }
  };

  const handleDelete = async (id: string) => {
    setActionSessionId(id);
    try {
      const result = await api.deleteSession(id);
      if (!result.success) {
        addToast({ type: 'error', message: result.error || `Failed to delete session ${id}` });
        return;
      }
      addToast({ type: 'success', message: `Deleted session ${id}` });
      await refreshSessions();
    } finally {
      setActionSessionId(null);
    }
  };

  return (
    <div className="flex flex-col h-full bg-slate-950">
      <div className="flex items-center justify-between px-6 py-4 border-b border-slate-900 sticky top-0 bg-slate-950/50 backdrop-blur-md z-10">
        <h2 className="text-xl font-bold text-white">Active Sessions</h2>
        <button
          onClick={refreshSessions}
          disabled={loading}
          aria-busy={loading}
          aria-label={loading ? 'Refreshing sessions...' : 'Refresh sessions'}
          title="Refresh sessions"
          className="p-2 rounded-lg hover:bg-slate-900 text-slate-400 hover:text-white transition disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
        >
          <RefreshCw size={18} className={loading ? 'animate-spin' : ''} aria-hidden="true" />
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-6" tabIndex={0} role="region" aria-label="Sessions">
        <div className="max-w-5xl mx-auto">
          {error ? (
            <ErrorPanel icon={XCircle} title="Connection Error" message={error} onRetry={refreshSessions} />
          ) : sessions.length === 0 ? (
            <EmptyState
              variant="card"
              icon={Clock}
              title="No active inference sessions"
              description="Start a chat to create a new session."
              action={{
                label: 'Start New Chat',
                icon: MessageSquare,
                onClick: () => setActiveTab(0),
              }}
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
                        <h3 className="font-bold text-slate-200">{session.name || session.id}</h3>
                        {session.name && <p className="text-xs text-slate-500">{session.id}</p>}
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
                        {session.status.toLowerCase() === 'saved' ? (
                          <>
                            <button
                              onClick={() => handleOpen(session.id)}
                              disabled={actionSessionId === session.id}
                              aria-busy={actionSessionId === session.id}
                              className="p-2 text-slate-500 hover:text-blue-400 hover:bg-blue-500/10 rounded-lg transition disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                              aria-label={
                                actionSessionId === session.id
                                  ? `Opening saved session ${session.id}...`
                                  : `Open saved session ${session.id}`
                              }
                              title={
                                actionSessionId === session.id
                                  ? `Opening saved session ${session.id}...`
                                  : `Open saved session ${session.id}`
                              }
                            >
                              {actionSessionId === session.id ? (
                                <Loader size={18} className="animate-spin text-blue-400" aria-hidden="true" />
                              ) : (
                                <FolderOpen size={18} aria-hidden="true" />
                              )}
                            </button>
                            <button
                              onClick={() => {
                                if (window.confirm(`Are you sure you want to delete saved session ${session.id}? This cannot be undone.`)) {
                                  handleDelete(session.id);
                                }
                              }}
                              disabled={actionSessionId === session.id}
                              aria-busy={actionSessionId === session.id}
                              className="p-2 text-slate-500 hover:text-red-400 hover:bg-red-500/10 rounded-lg transition disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                              aria-label={
                                actionSessionId === session.id
                                  ? `Deleting saved session ${session.id}...`
                                  : `Delete saved session ${session.id}`
                              }
                              title={
                                actionSessionId === session.id
                                  ? `Deleting saved session ${session.id}...`
                                  : `Delete saved session ${session.id}`
                              }
                            >
                              {actionSessionId === session.id ? (
                                <Loader size={18} className="animate-spin text-red-400" aria-hidden="true" />
                              ) : (
                                <Trash2 size={18} aria-hidden="true" />
                              )}
                            </button>
                          </>
                        ) : (
                          <button
                              onClick={() => {
                                if (window.confirm(`Are you sure you want to cancel session ${session.id}? This will immediately terminate the running inference.`)) {
                                  handleCancel(session.id);
                                }
                              }}
                              disabled={actionSessionId === session.id}
                              aria-busy={actionSessionId === session.id}
                              className="p-2 text-slate-500 hover:text-red-400 hover:bg-red-500/10 rounded-lg transition disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                              aria-label={
                                actionSessionId === session.id
                                  ? `Cancelling session ${session.id}...`
                                  : `Cancel session ${session.id}`
                              }
                              title={
                                actionSessionId === session.id
                                  ? `Cancelling session ${session.id}...`
                                  : `Cancel session ${session.id}`
                              }
                          >
                              {actionSessionId === session.id ? (
                                <Loader size={18} className="animate-spin text-red-400" aria-hidden="true" />
                              ) : (
                                <XCircle size={18} aria-hidden="true" />
                              )}
                          </button>
                        )}
                    </div>
                  </div>

                  <div className="grid grid-cols-3 gap-4">
                    <div className="bg-slate-950/50 p-4 rounded-xl border border-slate-800/50">
                        <p className="text-[10px] text-slate-500 uppercase font-bold tracking-tighter mb-1">Throughput</p>
                        <p className="text-xl font-bold text-blue-400">
                          {typeof session.throughput === "number" && session.throughput > 0 ? session.throughput : "—"}{" "}
                          <span className="text-xs font-normal text-slate-600">t/s</span>
                        </p>
                    </div>
                    <div className="bg-slate-950/50 p-4 rounded-xl border border-slate-800/50">
                        <p className="text-[10px] text-slate-500 uppercase font-bold tracking-tighter mb-1">Latency</p>
                        <p className="text-xl font-bold text-orange-400">
                          {typeof session.latency === "number" && session.latency > 0 ? session.latency : "—"}{" "}
                          <span className="text-xs font-normal text-slate-600">ms</span>
                        </p>
                    </div>
                    <div className="bg-slate-950/50 p-4 rounded-xl border border-slate-800/50">
                        <p className="text-[10px] text-slate-500 uppercase font-bold tracking-tighter mb-1">Tokens</p>
                        <p className="text-xl font-bold text-purple-400">
                          {typeof session.tokens === "number" && session.tokens > 0 ? session.tokens : "—"}
                        </p>
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
