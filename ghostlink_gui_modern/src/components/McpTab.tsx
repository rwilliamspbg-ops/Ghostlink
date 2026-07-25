import React, { useState, useEffect } from 'react';
import { RefreshCw, Plug, ShieldAlert } from 'lucide-react';
import { useAppStore } from '../store';
import { GhostlinkAPI } from '../api';
import { EmptyState } from './StatusViews';

export const McpTab: React.FC<{ api: GhostlinkAPI }> = ({ api }) => {
  const { mcpServers, setMcpServers } = useAppStore();
  const [loading, setLoading] = useState(false);
  const [toggling, setToggling] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = async () => {
    setLoading(true);
    const result = await api.listMcpServers();
    if (result.error) {
      setError(result.error);
    } else {
      setError(null);
      setMcpServers(result.servers);
    }
    setLoading(false);
  };

  useEffect(() => {
    refresh();
    const interval = setInterval(refresh, 10000);
    return () => clearInterval(interval);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [api]);

  const handleToggle = async (name: string, enabled: boolean) => {
    setToggling(name);
    const result = await api.toggleMcpServer(name, enabled);
    if (result.servers) {
      setMcpServers(result.servers);
    }
    if (!result.success && result.error) {
      setError(result.error);
    }
    setToggling(null);
  };

  return (
    <div className="flex flex-col h-full bg-slate-950">
      <div className="flex items-center justify-between px-6 py-4 border-b border-slate-900 sticky top-0 bg-slate-950/50 backdrop-blur-md z-10">
        <div className="flex items-center gap-3">
          <h2 className="text-xl font-bold text-white">MCP Servers</h2>
          <div className="px-2 py-0.5 bg-blue-600/20 text-blue-400 rounded text-[10px] font-bold uppercase tracking-wider">
            {mcpServers.filter((s) => s.connected).length} / {mcpServers.length} connected
          </div>
        </div>
        <button
          onClick={refresh}
          aria-label="Refresh MCP servers"
          className="p-2 rounded-lg hover:bg-slate-900 text-slate-400 hover:text-white transition"
        >
          <RefreshCw size={18} className={loading ? 'animate-spin' : ''} aria-hidden="true" />
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        <div className="max-w-4xl mx-auto">
          {error && (
            <div role="alert" className="mb-4 px-4 py-3 bg-red-500/10 border border-red-500/30 rounded-xl text-sm text-red-400">
              {error}
            </div>
          )}

          {mcpServers.length === 0 && !loading && (
            <EmptyState
              icon={Plug}
              title="No MCP servers configured"
              description={<>Add entries to <code className="text-slate-400">mcp_servers.toml</code> and refresh.</>}
            />
          )}

          <div className="grid grid-cols-1 gap-3">
            {mcpServers.map((server) => (
              <div
                key={server.name}
                className="bg-slate-900/50 border border-slate-800 rounded-2xl p-5 hover:border-slate-700 transition-all flex items-center gap-4"
              >
                <div
                  className={`p-3 rounded-xl transition-colors ${
                    server.connected ? 'bg-blue-600 text-white' : 'bg-slate-800 text-slate-500'
                  }`}
                >
                  <Plug size={20} />
                </div>

                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <h3 className="text-sm font-bold text-slate-100 truncate">{server.name}</h3>
                    {server.slot && (
                      <span className="px-1.5 py-0.5 bg-slate-800 text-slate-400 rounded text-[10px] font-mono">
                        {server.slot}
                      </span>
                    )}
                    {server.requires_confirmation && (
                      <span
                        className="flex items-center gap-1 px-1.5 py-0.5 bg-amber-500/10 text-amber-400 rounded text-[10px] font-bold uppercase tracking-wider"
                        title="Tool calls to this server require your approval before they run"
                      >
                        <ShieldAlert size={10} /> confirm
                      </span>
                    )}
                  </div>
                  <p className="text-xs text-slate-500 mt-0.5">
                    {server.connected
                      ? `${server.tool_count} tool${server.tool_count === 1 ? '' : 's'} available`
                      : server.enabled
                      ? 'Enabled but not connected'
                      : 'Disabled'}
                  </p>
                </div>

                <div
                  className={`flex items-center gap-2 px-3 py-1 rounded-full text-[10px] font-bold ${
                    server.connected
                      ? 'bg-green-500/10 text-green-400'
                      : server.enabled
                      ? 'bg-amber-500/10 text-amber-400'
                      : 'bg-slate-800 text-slate-500'
                  }`}
                >
                  <div
                    className={`w-1.5 h-1.5 rounded-full ${
                      server.connected ? 'bg-green-400 animate-pulse' : 'bg-slate-600'
                    }`}
                  ></div>
                  {server.connected ? 'Connected' : server.enabled ? 'Error' : 'Disabled'}
                </div>

                <button
                  onClick={() => handleToggle(server.name, !server.enabled)}
                  disabled={toggling === server.name}
                  role="switch"
                  aria-checked={server.enabled}
                  aria-label={`${server.enabled ? 'Disable' : 'Enable'} ${server.name}`}
                  className={`relative w-11 h-6 rounded-full transition-colors shrink-0 ${
                    server.enabled ? 'bg-blue-600' : 'bg-slate-700'
                  } ${toggling === server.name ? 'opacity-50' : ''}`}
                  title={server.enabled ? 'Disable server' : 'Enable server'}
                >
                  <div
                    aria-hidden="true"
                    className={`absolute top-0.5 w-5 h-5 bg-white rounded-full transition-transform ${
                      server.enabled ? 'translate-x-5' : 'translate-x-0.5'
                    }`}
                  ></div>
                </button>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
};
