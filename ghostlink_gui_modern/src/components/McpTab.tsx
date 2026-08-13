import React, { useEffect, useRef, useState } from 'react';
import { Pencil, Plug, Plus, RefreshCw, ShieldAlert, Trash2, X } from 'lucide-react';
import { McpServer, McpServerInput, useAppStore } from '../store';
import { GhostlinkAPI } from '../api';
import { EmptyState } from './StatusViews';

const EMPTY_FORM: McpServerInput = {
  name: '',
  slot: '',
  enabled: true,
  requires_confirmation: false,
  timeout_secs: 30,
  transport: 'stdio',
  command: '',
  args: [],
  env: {},
  url: '',
  headers: {},
};

// "KEY=value" per line <-> Record<string,string>, used for both env vars and
// HTTP headers so the form doesn't need a dynamic add/remove-row widget.
function parseKeyValueLines(text: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const line of text.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const eq = trimmed.indexOf('=');
    if (eq === -1) continue;
    out[trimmed.slice(0, eq).trim()] = trimmed.slice(eq + 1).trim();
  }
  return out;
}

function formatKeyValueLines(record: Record<string, string> | undefined): string {
  return Object.entries(record || {})
    .map(([k, v]) => `${k}=${v}`)
    .join('\n');
}

function serverToFormInput(server: McpServer): McpServerInput {
  return {
    name: server.name,
    slot: server.slot,
    enabled: server.enabled,
    requires_confirmation: server.requires_confirmation,
    timeout_secs: server.timeout_secs,
    transport: server.transport.transport,
    command: server.transport.transport === 'stdio' ? server.transport.command : '',
    args: server.transport.transport === 'stdio' ? server.transport.args : [],
    env: server.transport.transport === 'stdio' ? server.transport.env : {},
    url: server.transport.transport === 'http' ? server.transport.url : '',
    headers: server.transport.transport === 'http' ? server.transport.headers : {},
  };
}

export const McpTab: React.FC<{ api: GhostlinkAPI }> = ({ api }) => {
  const { mcpServers, setMcpServers, addToast } = useAppStore();
  const [loading, setLoading] = useState(false);
  const [toggling, setToggling] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  // null = closed, '' = "add new" mode, otherwise the name of the server being edited
  const [editorTarget, setEditorTarget] = useState<string | null>(null);
  const [form, setForm] = useState<McpServerInput>(EMPTY_FORM);
  const [envText, setEnvText] = useState('');
  const [argsText, setArgsText] = useState('');
  const [formError, setFormError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const editorCloseRef = useRef<HTMLButtonElement>(null);
  const editorTriggerRef = useRef<HTMLElement | null>(null);

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
    if (result.success) {
      addToast({ type: 'success', message: `MCP server "${name}" is now ${enabled ? 'enabled' : 'disabled'}.` });
    } else {
      const errText = result.error || 'Unknown error';
      setError(errText);
      addToast({ type: 'error', message: `Failed to ${enabled ? 'enable' : 'disable'} MCP server "${name}": ${errText}` });
    }
    setToggling(null);
  };

  const handleDelete = async (name: string) => {
    if (!window.confirm(`Remove MCP server "${name}"? This edits mcp_servers.toml and disconnects it immediately.`)) {
      return;
    }
    setDeleting(name);
    const result = await api.deleteMcpServer(name);
    if (result.servers) {
      setMcpServers(result.servers);
    }
    if (result.success) {
      addToast({ type: 'success', message: `Removed MCP server "${name}" successfully.` });
    } else {
      const errText = result.error || 'Unknown error';
      setError(errText);
      addToast({ type: 'error', message: `Failed to remove MCP server "${name}": ${errText}` });
    }
    setDeleting(null);
  };

  const openCreateEditor = (trigger: HTMLElement) => {
    editorTriggerRef.current = trigger;
    setForm(EMPTY_FORM);
    setEnvText('');
    setArgsText('');
    setFormError(null);
    setEditorTarget('');
  };

  const openEditEditor = (server: McpServer, trigger: HTMLElement) => {
    editorTriggerRef.current = trigger;
    const input = serverToFormInput(server);
    setForm(input);
    setEnvText(formatKeyValueLines(input.transport === 'stdio' ? input.env : input.headers));
    setArgsText((input.args || []).join('\n'));
    setFormError(null);
    setEditorTarget(server.name);
  };

  const closeEditor = () => {
    setEditorTarget(null);
    editorTriggerRef.current?.focus();
  };

  useEffect(() => {
    if (editorTarget === null) return;
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') closeEditor();
    };
    document.addEventListener('keydown', handleKey);
    editorCloseRef.current?.focus();
    return () => document.removeEventListener('keydown', handleKey);
  }, [editorTarget]);

  const handleSave = async () => {
    const name = form.name.trim();
    if (!name) {
      setFormError('Name is required.');
      return;
    }
    if (form.transport === 'stdio' && !form.command?.trim()) {
      setFormError('Command is required for a stdio server.');
      return;
    }
    if (form.transport === 'http' && !form.url?.trim()) {
      setFormError('URL is required for an HTTP server.');
      return;
    }

    const kv = parseKeyValueLines(envText);
    const payload: McpServerInput = {
      name,
      slot: form.slot.trim(),
      enabled: form.enabled,
      requires_confirmation: form.requires_confirmation,
      timeout_secs: form.timeout_secs,
      transport: form.transport,
      ...(form.transport === 'stdio'
        ? {
            command: form.command?.trim() || '',
            args: argsText
              .split('\n')
              .map((a) => a.trim())
              .filter((a) => a !== ''),
            env: kv,
          }
        : {
            url: form.url?.trim() || '',
            headers: kv,
          }),
    };

    setSaving(true);
    setFormError(null);
    const result =
      editorTarget === '' ? await api.createMcpServer(payload) : await api.updateMcpServer(editorTarget!, payload);
    setSaving(false);

    if (!result.success) {
      setFormError(result.error || 'Failed to save server.');
      addToast({ type: 'error', message: `Failed to save MCP server "${name}": ${result.error || 'Unknown error'}` });
      return;
    }
    if (result.servers) {
      setMcpServers(result.servers);
    }
    addToast({ type: 'success', message: `${editorTarget === '' ? 'Added' : 'Updated'} MCP server "${name}" successfully.` });
    closeEditor();
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
        <div className="flex items-center gap-2">
          <button
            onClick={(e) => openCreateEditor(e.currentTarget)}
            className="flex items-center gap-2 px-3 py-1.5 bg-blue-600 hover:bg-blue-500 text-white text-xs font-bold rounded-lg transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
          >
            <Plus size={14} aria-hidden="true" /> Add Server
          </button>
          <button
            onClick={refresh}
            aria-label="Refresh MCP servers"
            className="p-2 rounded-lg hover:bg-slate-900 text-slate-400 hover:text-white transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
          >
            <RefreshCw size={18} className={loading ? 'animate-spin' : ''} aria-hidden="true" />
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-6" tabIndex={0} role="region" aria-label="MCP servers">
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
              description="Click Add Server above, or hand-edit mcp_servers.toml and refresh."
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
                  } ${toggling === server.name ? 'opacity-50' : ''} focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none`}
                  title={server.enabled ? 'Disable server' : 'Enable server'}
                >
                  <div
                    aria-hidden="true"
                    className={`absolute top-0.5 w-5 h-5 bg-white rounded-full transition-transform ${
                      server.enabled ? 'translate-x-5' : 'translate-x-0.5'
                    }`}
                  ></div>
                </button>

                <button
                  onClick={(e) => openEditEditor(server, e.currentTarget)}
                  className="p-1.5 rounded-lg hover:bg-slate-800 text-slate-400 hover:text-white transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                  title="Edit server"
                  aria-label={`Edit ${server.name}`}
                >
                  <Pencil size={16} aria-hidden="true" />
                </button>
                <button
                  onClick={() => handleDelete(server.name)}
                  disabled={deleting === server.name}
                  className="p-1.5 rounded-lg hover:bg-slate-800 text-slate-400 hover:text-red-400 transition disabled:opacity-40 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                  title="Remove server"
                  aria-label={`Remove ${server.name}`}
                >
                  <Trash2 size={16} aria-hidden="true" />
                </button>
              </div>
            ))}
          </div>
        </div>
      </div>

      {editorTarget !== null && (
        // eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-static-element-interactions -- click-outside-to-dismiss backdrop; Escape (handled above) and the close button already cover keyboard/AT users
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4" onClick={closeEditor}>
          {/* eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-noninteractive-element-interactions -- onClick here only stops the backdrop's close-on-click from firing when clicking inside the dialog, it's not a control */}
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="mcp-editor-title"
            onClick={(e) => e.stopPropagation()}
            className="bg-slate-900 border border-slate-800 rounded-2xl max-w-lg w-full max-h-[85vh] overflow-hidden flex flex-col"
          >
            <div className="flex items-center justify-between p-4 border-b border-slate-800">
              <h3 id="mcp-editor-title" className="text-lg font-bold text-white">
                {editorTarget === '' ? 'Add MCP Server' : `Edit ${editorTarget}`}
              </h3>
              <button
                ref={editorCloseRef}
                onClick={closeEditor}
                className="text-slate-400 hover:text-white focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                aria-label="Close"
              >
                <X size={20} aria-hidden="true" />
              </button>
            </div>

            <div className="p-4 overflow-y-auto space-y-4">
              {formError && (
                <div role="alert" className="px-3 py-2 bg-red-500/10 border border-red-500/30 rounded-lg text-xs text-red-400">
                  {formError}
                </div>
              )}

              <div>
                <label htmlFor="mcp-name" className="block text-xs font-bold text-slate-400 mb-1">
                  Name <span className="text-red-500" title="Required">*</span>
                </label>
                <input
                  id="mcp-name"
                  type="text"
                  required
                  value={form.name}
                  onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))}
                  placeholder="e.g. filesystem-server"
                  title="Enter the unique name of the MCP server"
                  className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:ring-2 focus:ring-blue-500/50 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                />
              </div>

              <div>
                <label htmlFor="mcp-slot" className="block text-xs font-bold text-slate-400 mb-1">
                  Slot <span className="font-normal text-slate-500">(optional — e.g. web_search, calculator)</span>
                </label>
                <input
                  id="mcp-slot"
                  type="text"
                  value={form.slot}
                  onChange={(e) => setForm((f) => ({ ...f, slot: e.target.value }))}
                  placeholder="e.g. workspace_tools"
                  title="Assign an optional slot or tool category for this server"
                  className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:ring-2 focus:ring-blue-500/50 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                />
              </div>

              <fieldset>
                <legend className="block text-xs font-bold text-slate-400 mb-1">Transport</legend>
                <div className="flex gap-4" role="radiogroup" aria-label="Transport">
                  {(['stdio', 'http'] as const).map((t) => (
                    <label key={t} className="flex items-center gap-1.5 text-sm text-slate-300 cursor-pointer select-none">
                      <input
                        type="radio"
                        name="mcp-transport"
                        checked={form.transport === t}
                        onChange={() => setForm((f) => ({ ...f, transport: t }))}
                        className="accent-blue-600 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                      />
                      {t === 'stdio' ? 'Stdio (local command)' : 'HTTP'}
                    </label>
                  ))}
                </div>
              </fieldset>

              {form.transport === 'stdio' ? (
                <>
                  <div>
                    <label htmlFor="mcp-command" className="block text-xs font-bold text-slate-400 mb-1">
                      Command <span className="text-red-500" title="Required">*</span>
                    </label>
                    <input
                      id="mcp-command"
                      type="text"
                      required={form.transport === 'stdio'}
                      value={form.command}
                      onChange={(e) => setForm((f) => ({ ...f, command: e.target.value }))}
                      placeholder="e.g. npx"
                      title="Enter the local command executable name"
                      className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 font-mono focus:outline-none focus:ring-2 focus:ring-blue-500/50 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                    />
                  </div>
                  <div>
                    <label htmlFor="mcp-args" className="block text-xs font-bold text-slate-400 mb-1">
                      Args <span className="font-normal text-slate-500">(one per line)</span>
                    </label>
                    <textarea
                      id="mcp-args"
                      value={argsText}
                      onChange={(e) => setArgsText(e.target.value)}
                      rows={3}
                      placeholder={'-y\n@modelcontextprotocol/server-filesystem'}
                      title="Enter arguments to pass to the command, one per line"
                      className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 font-mono focus:outline-none focus:ring-2 focus:ring-blue-500/50 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none resize-y"
                    />
                  </div>
                  <div>
                    <label htmlFor="mcp-env" className="block text-xs font-bold text-slate-400 mb-1">
                      Environment <span className="font-normal text-slate-500">(KEY=value per line — use ${'{VAR_NAME}'} for secrets, never a literal)</span>
                    </label>
                    <textarea
                      id="mcp-env"
                      value={envText}
                      onChange={(e) => setEnvText(e.target.value)}
                      rows={3}
                      placeholder={'BRAVE_API_KEY=${BRAVE_API_KEY}'}
                      title="Environment variables for the stdio process, formatted as KEY=value on each line"
                      className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 font-mono focus:outline-none focus:ring-2 focus:ring-blue-500/50 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none resize-y"
                    />
                  </div>
                </>
              ) : (
                <>
                  <div>
                    <label htmlFor="mcp-url" className="block text-xs font-bold text-slate-400 mb-1">
                      URL <span className="text-red-500" title="Required">*</span>
                    </label>
                    <input
                      id="mcp-url"
                      type="text"
                      required={form.transport === 'http'}
                      value={form.url}
                      onChange={(e) => setForm((f) => ({ ...f, url: e.target.value }))}
                      placeholder="e.g. https://mcp.example.com/mcp"
                      title="Enter the remote HTTP/SSE MCP endpoint URL"
                      className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 font-mono focus:outline-none focus:ring-2 focus:ring-blue-500/50 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                    />
                  </div>
                  <div>
                    <label htmlFor="mcp-headers" className="block text-xs font-bold text-slate-400 mb-1">
                      Headers <span className="font-normal text-slate-500">(KEY=value per line — use ${'{VAR_NAME}'} for secrets, never a literal)</span>
                    </label>
                    <textarea
                      id="mcp-headers"
                      value={envText}
                      onChange={(e) => setEnvText(e.target.value)}
                      rows={3}
                      placeholder={'X-Api-Key=${REMOTE_TOOLS_API_KEY}'}
                      title="Custom HTTP headers to include in remote calls, formatted as KEY=value on each line"
                      className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 font-mono focus:outline-none focus:ring-2 focus:ring-blue-500/50 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none resize-y"
                    />
                  </div>
                </>
              )}

              <div>
                <label htmlFor="mcp-timeout" className="block text-xs font-bold text-slate-400 mb-1">Timeout (seconds)</label>
                <input
                  id="mcp-timeout"
                  type="number"
                  min={1}
                  value={form.timeout_secs}
                  onChange={(e) => setForm((f) => ({ ...f, timeout_secs: Math.max(1, Number(e.target.value) || 1) }))}
                  title="Request timeout in seconds"
                  className="w-32 bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:ring-2 focus:ring-blue-500/50 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                />
              </div>

              <div className="flex items-center gap-4">
                <label className="flex items-center gap-2 text-sm text-slate-300 cursor-pointer select-none">
                  <input
                    type="checkbox"
                    checked={form.enabled}
                    onChange={(e) => setForm((f) => ({ ...f, enabled: e.target.checked }))}
                    className="accent-blue-600 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                  />
                  Enabled
                </label>
                <label className="flex items-center gap-2 text-sm text-slate-300 cursor-pointer select-none">
                  <input
                    type="checkbox"
                    checked={form.requires_confirmation}
                    onChange={(e) => setForm((f) => ({ ...f, requires_confirmation: e.target.checked }))}
                    className="accent-blue-600 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                  />
                  Require confirmation before tool calls
                </label>
              </div>
            </div>

            <div className="flex justify-end gap-2 p-4 border-t border-slate-800">
              <button
                onClick={closeEditor}
                className="px-4 py-2 bg-slate-800 text-white rounded-lg font-medium hover:bg-slate-700 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
              >
                Cancel
              </button>
              <button
                onClick={handleSave}
                disabled={saving}
                className="px-4 py-2 bg-blue-600 text-white rounded-lg font-medium hover:bg-blue-700 disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
              >
                {saving ? 'Saving...' : 'Save'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
