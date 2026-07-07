import React, { useState, useEffect } from 'react';
import { Send, Loader, ChevronDown, Plus, X, Wand2 } from 'lucide-react';
import { useAppStore, Model } from '../store';

interface ChatTabProps {
  api: any;
}

interface Tool {
  name: string;
  description: string;
  enabled: boolean;
}

interface MCPServer {
  id: string;
  name: string;
  url: string;
  enabled: boolean;
}

const AVAILABLE_TOOLS: Tool[] = [
  { name: 'web_search', description: 'Search the web for information', enabled: false },
  { name: 'calculator', description: 'Perform mathematical calculations', enabled: false },
  { name: 'code_execution', description: 'Execute Python code safely', enabled: false },
  { name: 'file_operations', description: 'Read and write files', enabled: false },
  { name: 'terminal', description: 'Execute terminal commands', enabled: false },
  { name: 'database_query', description: 'Query databases', enabled: false },
  { name: 'api_call', description: 'Make HTTP API calls', enabled: false },
  { name: 'image_generation', description: 'Generate and manipulate images', enabled: false },
];

export const ChatTab: React.FC<ChatTabProps> = ({ api }) => {
  const { currentModel, models, selectedModel, setSelectedModel, setModels } = useAppStore();
  const [message, setMessage] = useState('');
  const [response, setResponse] = useState('');
  const [loading, setLoading] = useState(false);
  const [temperature, setTemperature] = useState(0.7);
  const [topP, setTopP] = useState(0.9);
  const [topK, setTopK] = useState(40);
  const [penalty, setPenalty] = useState(1.1);
  const [maxTokens, setMaxTokens] = useState(256);
  const [systemPrompt, setSystemPrompt] = useState(
    'You are a highly capable AI assistant running on Ghostlink Fabric.'
  );
  const [error, setError] = useState('');
  const [showTools, setShowTools] = useState(false);
  const [tools, setTools] = useState<Tool[]>(AVAILABLE_TOOLS);
  const [mcpServers, setMcpServers] = useState<MCPServer[]>([]);
  const [newMcpName, setNewMcpName] = useState('');
  const [newMcpUrl, setNewMcpUrl] = useState('');
  const [showMcpForm, setShowMcpForm] = useState(false);

  // Fetch models on component mount
  useEffect(() => {
    const fetchModels = async () => {
      const result = await api.getModels();
      if (!result.error) {
        setModels(result.models);
      }
    };
    fetchModels();
  }, [api, setModels]);

  const handleSend = async () => {
    if (!message.trim()) {
      setError('Enter a message first.');
      return;
    }

    if (!selectedModel) {
      setError('Select a model first.');
      return;
    }

    setLoading(true);
    setError('');
    setResponse('');

    // Build tools configuration
    const enabledTools = tools.filter((t) => t.enabled).map((t) => t.name);
    const enabledMcpServers = mcpServers.filter((s) => s.enabled);

    const payload: any = {
      message: message.trim(),
      temperature,
      top_p: topP,
      top_k: topK,
      penalty,
      max_tokens: maxTokens,
      system_prompt: systemPrompt,
      stream: true,
    };

    // Add tools if any are enabled
    if (enabledTools.length > 0) {
      payload.tools = enabledTools;
    }

    // Add MCP servers if any are enabled
    if (enabledMcpServers.length > 0) {
      payload.mcp_servers = enabledMcpServers.map((s) => ({
        name: s.name,
        url: s.url,
      }));
    }

    let firstToken = true;
    const result = await api.sendMessage(payload, (token: string) => {
      if (firstToken) {
        setResponse('Assistant: ');
        firstToken = false;
      }
      setResponse((prev) => prev + token);
    });
    setLoading(false);

    if (result.success) {
      if (!payload.stream) {
        setResponse(`Assistant: ${result.data.response || ''}`);
      }
      if (result.data.request_id) {
        setResponse((prev) => `${prev}\n\nRequest: ${result.data.request_id}`);
      }
      if (result.data.tools_used) {
        setResponse((prev) => `${prev}\n\nTools used: ${result.data.tools_used.join(', ')}`);
      }
      setMessage('');
    } else {
      setError(result.error || 'Failed to send message.');
    }
  };

  const usableModels = models.filter((m) => m.usable && m.status === 'ready');
  const enabledToolCount = tools.filter((t) => t.enabled).length;
  const enabledMcpCount = mcpServers.filter((s) => s.enabled).length;

  const toggleTool = (toolName: string) => {
    setTools(tools.map((t) => (t.name === toolName ? { ...t, enabled: !t.enabled } : t)));
  };

  const addMcpServer = () => {
    if (!newMcpName.trim() || !newMcpUrl.trim()) {
      setError('MCP name and URL required');
      return;
    }

    const newServer: MCPServer = {
      id: `mcp-${Date.now()}`,
      name: newMcpName,
      url: newMcpUrl,
      enabled: true,
    };

    setMcpServers([...mcpServers, newServer]);
    setNewMcpName('');
    setNewMcpUrl('');
    setShowMcpForm(false);
  };

  const removeMcpServer = (id: string) => {
    setMcpServers(mcpServers.filter((s) => s.id !== id));
  };

  const toggleMcpServer = (id: string) => {
    setMcpServers(mcpServers.map((s) => (s.id === id ? { ...s, enabled: !s.enabled } : s)));
  };

  return (
    <div className="grid grid-cols-2 gap-6 h-full">
      {/* Left side - Input */}
      <div className="flex flex-col gap-4 overflow-y-auto">
        {/* Model Selector */}
        <div>
          <label className="block text-sm font-semibold text-slate-200 mb-2">Active Model</label>
          <div className="relative">
            <select
              value={selectedModel || ''}
              onChange={(e) => setSelectedModel(e.target.value || null)}
              className="w-full px-4 py-2 bg-slate-800 border border-slate-700 rounded text-slate-100 focus:outline-none focus:border-blue-500 appearance-none cursor-pointer"
            >
              <option value="">
                {usableModels.length === 0
                  ? 'No models available - Go to Models tab'
                  : 'Select a model...'}
              </option>
              {usableModels.map((m) => (
                <option key={m.name} value={m.name}>
                  {m.name} ({m.size_gb.toFixed(1)}GB)
                </option>
              ))}
            </select>
            <ChevronDown className="absolute right-3 top-2.5 text-slate-400 pointer-events-none" size={18} />
          </div>
          {selectedModel && (
            <p className="text-xs text-emerald-400 mt-1">✓ {selectedModel} ready</p>
          )}
        </div>

        {/* Prompt */}
        <div>
          <label className="block text-sm font-semibold text-slate-200 mb-2">Prompt</label>
          <textarea
            value={message}
            onChange={(e) => setMessage(e.target.value)}
            placeholder="Enter your message..."
            className="w-full h-20 p-3 bg-slate-800 border border-slate-700 rounded text-slate-100 placeholder-slate-500 focus:outline-none focus:border-blue-500 resize-none"
          />
        </div>

        {/* Tools & MCP Section */}
        <div className="bg-slate-900 rounded p-4 border border-slate-700 space-y-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Wand2 size={16} className="text-purple-400" />
              <h3 className="text-sm font-semibold text-slate-200">Tools & MCP</h3>
              {enabledToolCount + enabledMcpCount > 0 && (
                <span className="px-2 py-1 bg-purple-600 text-white text-xs rounded">
                  {enabledToolCount + enabledMcpCount} active
                </span>
              )}
            </div>
            <button
              onClick={() => setShowTools(!showTools)}
              className="text-xs text-slate-400 hover:text-slate-300"
            >
              {showTools ? 'Hide' : 'Show'}
            </button>
          </div>

          {showTools && (
            <div className="space-y-3">
              {/* Tools */}
              <div>
                <p className="text-xs text-slate-400 mb-2">Tools:</p>
                <div className="space-y-1 bg-slate-800 p-2 rounded max-h-32 overflow-y-auto">
                  {tools.map((tool) => (
                    <label key={tool.name} className="flex items-center gap-2 text-xs text-slate-300 cursor-pointer hover:text-slate-200">
                      <input
                        type="checkbox"
                        checked={tool.enabled}
                        onChange={() => toggleTool(tool.name)}
                        className="w-3 h-3 cursor-pointer"
                      />
                      <span>{tool.name}</span>
                      <span className="text-slate-500 text-xs">({tool.description})</span>
                    </label>
                  ))}
                </div>
              </div>

              {/* MCP Servers */}
              <div>
                <div className="flex items-center justify-between mb-2">
                  <p className="text-xs text-slate-400">MCP Servers:</p>
                  <button
                    onClick={() => setShowMcpForm(!showMcpForm)}
                    className="text-xs text-blue-400 hover:text-blue-300 flex items-center gap-1"
                  >
                    <Plus size={12} />
                    Add
                  </button>
                </div>

                {showMcpForm && (
                  <div className="bg-slate-800 p-2 rounded mb-2 space-y-2">
                    <input
                      type="text"
                      placeholder="Server name"
                      value={newMcpName}
                      onChange={(e) => setNewMcpName(e.target.value)}
                      className="w-full px-2 py-1 bg-slate-700 border border-slate-600 rounded text-xs text-slate-100 focus:outline-none"
                    />
                    <input
                      type="text"
                      placeholder="Server URL (e.g., http://localhost:5000)"
                      value={newMcpUrl}
                      onChange={(e) => setNewMcpUrl(e.target.value)}
                      className="w-full px-2 py-1 bg-slate-700 border border-slate-600 rounded text-xs text-slate-100 focus:outline-none"
                    />
                    <div className="flex gap-2">
                      <button
                        onClick={addMcpServer}
                        className="flex-1 px-2 py-1 bg-blue-600 hover:bg-blue-700 text-white text-xs rounded transition"
                      >
                        Add
                      </button>
                      <button
                        onClick={() => setShowMcpForm(false)}
                        className="flex-1 px-2 py-1 bg-slate-700 hover:bg-slate-600 text-white text-xs rounded transition"
                      >
                        Cancel
                      </button>
                    </div>
                  </div>
                )}

                <div className="space-y-1 bg-slate-800 p-2 rounded max-h-32 overflow-y-auto">
                  {mcpServers.length === 0 ? (
                    <p className="text-xs text-slate-500 italic">No MCP servers added</p>
                  ) : (
                    mcpServers.map((server) => (
                      <div key={server.id} className="flex items-center justify-between text-xs text-slate-300 bg-slate-700 p-1 rounded">
                        <label className="flex items-center gap-2 cursor-pointer flex-1">
                          <input
                            type="checkbox"
                            checked={server.enabled}
                            onChange={() => toggleMcpServer(server.id)}
                            className="w-3 h-3 cursor-pointer"
                          />
                          <div>
                            <div className="font-medium">{server.name}</div>
                            <div className="text-slate-500 text-xs">{server.url}</div>
                          </div>
                        </label>
                        <button
                          onClick={() => removeMcpServer(server.id)}
                          className="text-red-400 hover:text-red-300 p-1"
                        >
                          <X size={12} />
                        </button>
                      </div>
                    ))
                  )}
                </div>
              </div>
            </div>
          )}
        </div>

        {/* Parameters */}
        <div className="bg-slate-900 rounded p-4 space-y-3">
          <h3 className="text-sm font-semibold text-slate-200">Parameters</h3>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs text-slate-400 mb-1">Temperature ({temperature.toFixed(2)})</label>
              <input
                type="range"
                min="0"
                max="2"
                step="0.1"
                value={temperature}
                onChange={(e) => setTemperature(parseFloat(e.target.value))}
                className="w-full"
              />
            </div>
            <div>
              <label className="block text-xs text-slate-400 mb-1">Top P ({topP.toFixed(2)})</label>
              <input
                type="range"
                min="0"
                max="1"
                step="0.05"
                value={topP}
                onChange={(e) => setTopP(parseFloat(e.target.value))}
                className="w-full"
              />
            </div>
            <div>
              <label className="block text-xs text-slate-400 mb-1">Top K ({topK})</label>
              <input
                type="range"
                min="0"
                max="100"
                value={topK}
                onChange={(e) => setTopK(parseInt(e.target.value))}
                className="w-full"
              />
            </div>
            <div>
              <label className="block text-xs text-slate-400 mb-1">Penalty ({penalty.toFixed(2)})</label>
              <input
                type="range"
                min="1"
                max="2"
                step="0.05"
                value={penalty}
                onChange={(e) => setPenalty(parseFloat(e.target.value))}
                className="w-full"
              />
            </div>
          </div>
          <div>
            <label className="block text-xs text-slate-400 mb-1">Max Tokens ({maxTokens})</label>
            <input
              type="range"
              min="1"
              max="32768"
              value={maxTokens}
              onChange={(e) => setMaxTokens(parseInt(e.target.value))}
              className="w-full"
            />
          </div>
        </div>

        {/* System Prompt */}
        <div>
          <label className="block text-sm font-semibold text-slate-200 mb-2">System Prompt</label>
          <textarea
            value={systemPrompt}
            onChange={(e) => setSystemPrompt(e.target.value)}
            className="w-full h-20 p-3 bg-slate-800 border border-slate-700 rounded text-slate-100 placeholder-slate-500 focus:outline-none focus:border-blue-500 resize-none"
          />
        </div>

        {/* Send Button */}
        <button
          onClick={handleSend}
          disabled={loading || usableModels.length === 0}
          className="w-full flex items-center justify-center gap-2 bg-blue-600 hover:bg-blue-700 disabled:bg-slate-600 text-white font-semibold py-3 rounded transition"
        >
          {loading ? (
            <>
              <Loader size={18} className="animate-spin" />
              Sending...
            </>
          ) : (
            <>
              <Send size={18} />
              Send Message
            </>
          )}
        </button>

        {error && <div className="text-red-400 text-sm bg-red-900 bg-opacity-20 p-2 rounded">{error}</div>}
      </div>

      {/* Right side - Response */}
      <div className="flex flex-col gap-4">
        <h3 className="text-sm font-semibold text-slate-200">Response</h3>
        <div className="flex-1 bg-slate-900 border border-slate-700 rounded p-4 overflow-y-auto">
          {loading && !response ? (
            <div className="flex items-center justify-center h-full text-slate-400">
              <Loader className="animate-spin mr-2" />
              Waiting for response...
            </div>
          ) : response ? (
            <pre className="text-slate-300 whitespace-pre-wrap break-words font-mono text-sm">{response}</pre>
          ) : (
            <p className="text-slate-500 italic">Response will appear here...</p>
          )}
        </div>
      </div>
    </div>
  );
};
