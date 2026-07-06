import React, { useState, useEffect } from 'react';
import { RefreshCw, Trash2 } from 'lucide-react';
import { useAppStore } from '../store';

interface SessionsTabProps {
  api: any;
}

export const SessionsTab: React.FC<SessionsTabProps> = ({ api }) => {
  const { sessions, setSessions } = useAppStore();
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState('');

  const refreshSessions = async () => {
    setLoading(true);
    const result = await api.getSessions();
    if (!result.error) {
      setSessions(result.sessions);
    }
    setLoading(false);
  };

  useEffect(() => {
    refreshSessions();
    const interval = setInterval(refreshSessions, 5000);
    return () => clearInterval(interval);
  }, []);

  const handleCancelSession = async (sessionId: string) => {
    const result = await api.cancelSession(sessionId);
    if (result.success) {
      setMessage('Session cancelled');
      setTimeout(() => refreshSessions(), 500);
      setTimeout(() => setMessage(''), 3000);
    } else {
      setMessage(`Error: ${result.error}`);
    }
  };

  return (
    <div className="space-y-4">
      {/* Controls */}
      <div className="flex gap-2">
        <button
          onClick={refreshSessions}
          disabled={loading}
          className="flex items-center gap-2 px-4 py-2 bg-slate-700 hover:bg-slate-600 disabled:bg-slate-800 text-white rounded transition"
        >
          <RefreshCw size={16} className={loading ? 'animate-spin' : ''} />
          Refresh
        </button>
      </div>

      {message && (
        <div className="p-3 rounded bg-emerald-900 text-emerald-200">{message}</div>
      )}

      {/* Sessions Table */}
      <div className="overflow-x-auto rounded border border-slate-700">
        <table className="w-full">
          <thead className="bg-slate-900 border-b border-slate-700">
            <tr>
              <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">ID</th>
              <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Model</th>
              <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Status</th>
              <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Throughput</th>
              <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Latency (ms)</th>
              <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Tokens</th>
              <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Action</th>
            </tr>
          </thead>
          <tbody>
            {sessions.length === 0 ? (
              <tr>
                <td colSpan={7} className="px-4 py-4 text-center text-slate-500">
                  No active sessions
                </td>
              </tr>
            ) : (
              sessions.map((session) => (
                <tr key={session.id} className="border-b border-slate-700 hover:bg-slate-800">
                  <td className="px-4 py-3 text-sm text-slate-200 font-mono">{session.id.slice(0, 8)}...</td>
                  <td className="px-4 py-3 text-sm text-slate-200">{session.model}</td>
                  <td className="px-4 py-3 text-sm">
                    <span
                      className={`px-2 py-1 rounded text-xs font-semibold ${
                        session.status === 'running'
                          ? 'bg-blue-900 text-blue-200'
                          : 'bg-slate-700 text-slate-300'
                      }`}
                    >
                      {session.status}
                    </span>
                  </td>
                  <td className="px-4 py-3 text-sm text-slate-400">{session.throughput.toFixed(2)} req/s</td>
                  <td className="px-4 py-3 text-sm text-slate-400">{session.latency.toFixed(1)}</td>
                  <td className="px-4 py-3 text-sm text-slate-400">{session.tokens}</td>
                  <td className="px-4 py-3 text-sm">
                    <button
                      onClick={() => handleCancelSession(session.id)}
                      className="text-red-400 hover:text-red-300"
                    >
                      <Trash2 size={16} />
                    </button>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
};
