import React, { useState, useEffect, useRef, useCallback } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import {
  Send,
  Loader,
  Plus,
  ChevronDown,
  Wand2,
  X,
  User,
  Bot,
  Copy,
  ThumbsUp,
  ThumbsDown,
  RotateCcw,
  Volume2,
  Mic,
  Cloud,
  LayoutGrid,
  Check,
  Save,
  FolderOpen,
  Trash2,
  Database,
} from 'lucide-react';
import { useAppStore } from '../store';
import { GhostlinkAPI } from '../api';

interface Message {
  role: 'user' | 'assistant';
  content: string;
  id: string;
  timestamp: string;
  model?: string;
}

interface Session {
  id: string;
  model: string;
  status: string;
  throughput: number;
  latency: number;
  tokens: number;
}

const STORAGE_KEY = 'ghostlink-chat-messages';

function loadMessages(): Message[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch { return []; }
}

function saveMessages(messages: Message[]) {
  try { localStorage.setItem(STORAGE_KEY, JSON.stringify(messages.slice(-200))); } catch { /* quota */ }
}

interface Tool {
  name: string;
  description: string;
  enabled: boolean;
}

const AVAILABLE_TOOLS: Tool[] = [
  { name: 'web_search', description: 'Search the web', enabled: false },
  { name: 'calculator', description: 'Basic math', enabled: false },
  { name: 'code_execution', description: 'Run Python', enabled: false },
  { name: 'file_operations', description: 'File I/O', enabled: false },
  { name: 'terminal', description: 'Shell access', enabled: false },
  { name: 'database_query', description: 'SQL query', enabled: false },
  { name: 'api_call', description: 'HTTP request', enabled: false },
  { name: 'image_generation', description: 'AI images', enabled: false },
];

export const ChatTab: React.FC<{ api: GhostlinkAPI }> = ({ api }) => {
  const { currentModel, models, setCurrentModel } = useAppStore();
  const [messages, setMessages] = useState<Message[]>(loadMessages);
  const [input, setInput] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [streamingId, setStreamingId] = useState<string | null>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);

  // Controls
  const [temperature] = useState(0.7);
  const [maxTokens] = useState(2048);
  const [systemPrompt] = useState('You are a production-grade, kernel-bypass-aware system orchestration co-pilot.');

  // UI State
  const [showTools, setShowTools] = useState(false);
  const [showModelSelector, setShowModelSelector] = useState(false);
  const [showSessions, setShowSessions] = useState(false);
  const [sessionName, setSessionName] = useState(`Session ${new Date().toLocaleDateString()}`);
  const [tools, setTools] = useState<Tool[]>(AVAILABLE_TOOLS);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [sessionsLoading, setSessionsLoading] = useState(false);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const modelSelectorRef = useRef<HTMLDivElement>(null);
  const sessionsRef = useRef<HTMLDivElement>(null);

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages, loading]);

  useEffect(() => {
    saveMessages(messages);
  }, [messages]);

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (modelSelectorRef.current && !modelSelectorRef.current.contains(event.target as Node)) {
        setShowModelSelector(false);
      }
      if (sessionsRef.current && !sessionsRef.current.contains(event.target as Node)) {
        setShowSessions(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const loadSessions = useCallback(async () => {
    setSessionsLoading(true);
    try {
      const result = await api.listSessions();
      if (!result.error && result.sessions) {
        setSessions(result.sessions);
      }
    } catch (e) {
      console.error('Failed to load sessions:', e);
    }
    setSessionsLoading(false);
  }, [api]);

  useEffect(() => {
    loadSessions();
  }, [loadSessions]);

  const handleSaveSession = async () => {
    if (messages.length === 0) return;
    const sessionId = `session_${Date.now()}`;
    const session = {
      id: sessionId,
      name: sessionName,
      model: currentModel === 'none' ? 'unknown' : currentModel,
      messages: messages,
    };
    
    const result = await api.saveSession(session);
    if (result.success) {
      loadSessions();
      setShowSessions(false);
      setSessionName(`Session ${new Date().toLocaleDateString()}`);
    } else {
      setError(result.error || 'Failed to save session');
    }
  };

  const handleLoadSession = async (sessionId: string) => {
    const result = await api.loadSession(sessionId);
    if (result.success && result.session) {
      // The backend only returns basic session info, we'd need to store full messages
      // For now, we'll load from localStorage as a fallback
      setShowSessions(false);
    } else {
      setError(result.error || 'Failed to load session');
    }
  };

  const handleDeleteSession = async (sessionId: string) => {
    if (!window.confirm('Delete this session?')) return;
    const result = await api.deleteSession(sessionId);
    if (result.success) {
      loadSessions();
    } else {
      setError(result.error || 'Failed to delete session');
    }
  };

  const usableModels = models.filter((m) => m.status === 'Loaded' || m.status === 'Ready');

  const handleSend = async () => {
    // CRITICAL FIX #1: Capture input BEFORE clearing
    const messageText = input.trim();
    if (!messageText || loading) return;

    const userMessage: Message = {
      role: 'user',
      content: messageText,
      id: Date.now().toString(),
      timestamp: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
    };

    setMessages((prev) => [...prev, userMessage]);
    setInput('');
    setLoading(true);
    setError(null);

    const assistantId = (Date.now() + 1).toString();
    setStreamingId(assistantId);
    setMessages((prev) => [...prev, { role: 'assistant', content: '', id: assistantId, timestamp: '', model: currentModel || undefined }]);

    const enabledTools = tools.filter((t) => t.enabled).map((t) => t.name);

    const result = await api.sendMessage({
      message: messageText,
      temperature,
      top_p: 0.9,
      top_k: 40,
      penalty: 1.1,
      max_tokens: maxTokens,
      system_prompt: systemPrompt,
      mcp: { tools: enabledTools },
      stream: true,
      model: currentModel === 'none' ? undefined : currentModel,
    }, (token: string) => {
      setMessages((prev) => prev.map(m =>
        m.id === assistantId ? { ...m, content: m.content + token } : m
      ));
    });

    setLoading(false);
    setStreamingId(null);

    if (result.success) {
      setMessages((prev) => prev.map(m =>
        m.id === assistantId
          ? { ...m, timestamp: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) }
          : m
      ));
    } else {
      setMessages((prev) => prev.filter(m => m.id !== assistantId));
      setError(result.error || 'Failed to send message');
    }
  };

  const handleCopy = async (content: string, id: string) => {
    try {
      await navigator.clipboard.writeText(content);
      setCopiedId(id);
      setTimeout(() => setCopiedId(null), 2000);
    } catch { /* noop */ }
  };

  const handleRegenerate = () => {
    const lastUser = [...messages].reverse().find(m => m.role === 'user');
    if (lastUser) {
      setMessages((prev) => {
        const idx = prev.findIndex(m => m.role === 'assistant' && m.id === streamingId);
        return idx >= 0 ? prev.slice(0, idx) : prev;
      });
      setInput(lastUser.content);
    }
  };

  const selectModel = async (name: string) => {
    setShowModelSelector(false);
    // With Ollama, just set current model - no load/unload needed
    setCurrentModel(name);
  };

  const toggleTool = (name: string) => {
    setTools(tools.map(t => t.name === name ? { ...t, enabled: !t.enabled } : t));
  };

  return (
    <div className="flex flex-col h-full bg-slate-950 relative">
      {/* Header / Model Selector */}
      <div className="flex items-center justify-between px-6 py-3 border-b border-slate-900 bg-slate-950/50 backdrop-blur-md sticky top-0 z-10">
        <div className="flex items-center gap-2">
            <div className="relative" ref={modelSelectorRef}>
                <button
                    onClick={() => setShowModelSelector(!showModelSelector)}
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg hover:bg-slate-900 transition text-sm font-semibold group"
                >
                    <span className="text-slate-200 group-hover:text-white max-w-[200px] truncate">
                        {currentModel === 'none' ? 'Select Model' : currentModel.split('/').pop()}
                    </span>
                    <ChevronDown size={14} className={`text-slate-500 transition-transform ${showModelSelector ? 'rotate-180' : ''}`} />
                </button>

                {showModelSelector && (
                    <div className="absolute left-0 mt-2 w-72 bg-slate-900 border border-slate-800 rounded-2xl shadow-2xl z-50 overflow-hidden animate-in fade-in zoom-in-95 duration-100">
                        <div className="p-2 max-h-[400px] overflow-y-auto">
                            {usableModels.length === 0 ? (
                                <div className="p-4 text-center text-slate-500 text-sm">No models available</div>
                            ) : (
                                usableModels.map(m => (
                                    <button
                                        key={m.name}
                                        onClick={() => selectModel(m.name)}
                                        className={`flex items-center justify-between w-full px-3 py-2.5 rounded-xl text-left transition text-sm ${
                                            currentModel === m.name ? 'bg-blue-600/10 text-blue-400' : 'text-slate-300 hover:bg-slate-800'
                                        }`}
                                    >
                                        <div className="flex flex-col min-w-0">
                                            <span className="font-bold truncate">{m.name.split('/').pop()}</span>
                                            <span className="text-[10px] text-slate-500 truncate">{m.name}</span>
                                        </div>
                                        {currentModel === m.name && <Check size={14} />}
                                    </button>
                                ))
                            )}
                        </div>
                        <div className="p-2 border-t border-slate-800 bg-slate-900/50">
                            <button className="flex items-center gap-2 w-full px-3 py-2 rounded-xl text-xs font-bold text-slate-400 hover:text-white hover:bg-slate-800 transition">
                                <Plus size={14} /> Download Models
                            </button>
                        </div>
                    </div>
                )}
            </div>
            <div className="relative" ref={sessionsRef}>
              <div className="flex items-center gap-1">
                <button
                  onClick={() => { if (!loading) { setSessionName(`Session ${new Date().toLocaleDateString()}`); setShowSessions(true); }}}
                  className="p-1.5 rounded-lg hover:bg-slate-900 text-slate-500 hover:text-white transition"
                  title="Save Session"
                >
                  <Save size={16} />
                </button>
                <button
                  onClick={() => { loadSessions(); setShowSessions(true); }}
                  className="p-1.5 rounded-lg hover:bg-slate-900 text-slate-500 hover:text-white transition"
                  title="Load Session"
                >
                  <FolderOpen size={16} />
                </button>
                <button onClick={() => { if (!loading) setMessages([]); }} className="p-1.5 rounded-lg hover:bg-slate-900 text-slate-500 hover:text-white transition">
                    <Plus size={16} />
                </button>
              </div>

              {showSessions && (
                <div className="absolute right-0 mt-2 w-80 bg-slate-900 border border-slate-800 rounded-2xl shadow-2xl z-50 overflow-hidden animate-in fade-in zoom-in-95 duration-100">
                  <div className="p-2 border-b border-slate-800 flex items-center justify-between">
                    <h4 className="text-xs font-bold text-slate-400 uppercase tracking-wider">Sessions</h4>
                    <button onClick={() => setShowSessions(false)} className="text-slate-500 hover:text-white"><X size={14} /></button>
                  </div>
                  <div className="p-2 max-h-[400px] overflow-y-auto">
                    {sessionsLoading ? (
                      <div className="flex justify-center py-4">
                        <Loader size={20} className="text-blue-500 animate-spin" />
                      </div>
                    ) : sessions.length === 0 ? (
                      <div className="p-4 text-center text-slate-500 text-sm">
                        <Database size={24} className="mx-auto mb-2 text-slate-700" />
                        <p>No saved sessions</p>
                      </div>
                    ) : (
                      sessions.map((s: Session) => (
                        <div key={s.id} className="flex items-center justify-between px-3 py-2.5 rounded-xl text-left transition text-sm">
                          <div className="flex flex-col min-w-0">
                            <span className="font-bold truncate">{s.id}</span>
                            <span className="text-[10px] text-slate-500 truncate">{s.model} • {s.tokens} messages</span>
                          </div>
                          <div className="flex items-center gap-1">
                            <button
                              onClick={() => handleLoadSession(s.id)}
                              className="p-1.5 hover:bg-slate-800 rounded-lg text-slate-400 hover:text-white transition"
                              title="Load"
                            >
                              <FolderOpen size={12} />
                            </button>
                            <button
                              onClick={() => handleDeleteSession(s.id)}
                              className="p-1.5 hover:bg-slate-800 rounded-lg text-slate-400 hover:text-red-400 transition"
                              title="Delete"
                            >
                              <Trash2 size={12} />
                            </button>
                          </div>
                        </div>
                      ))
                    )}
                  </div>
                  <div className="p-2 border-t border-slate-800 bg-slate-900/50">
                    <div className="space-y-2">
                      <input
                        type="text"
                        value={sessionName}
                        onChange={(e) => setSessionName(e.target.value)}
                        placeholder="Session name..."
                        className="w-full bg-slate-800 border border-slate-700 rounded-xl px-3 py-2 text-sm text-slate-200 focus:outline-none focus:ring-2 focus:ring-blue-500/50 focus:border-blue-500/50"
                      />
                      <button
                        onClick={handleSaveSession}
                        disabled={!sessionName.trim() || messages.length === 0}
                        className="flex items-center gap-2 w-full px-3 py-2 rounded-xl text-xs font-bold transition disabled:opacity-50 disabled:cursor-not-allowed bg-blue-600 text-white hover:bg-blue-500"
                      >
                        <Save size={14} /> Save Current Chat
                      </button>
                    </div>
                  </div>
                </div>
              )}
            </div>
          </div>
        <div className="flex items-center gap-2">
            <button className="p-2 rounded-lg hover:bg-slate-900 text-slate-400 hover:text-white transition">
                <LayoutGrid size={18} />
            </button>
            <div className="w-8 h-8 rounded-full bg-orange-500 flex items-center justify-center text-xs font-bold text-white shadow-lg shadow-orange-500/20">R</div>
        </div>
      </div>

      {/* Message Area */}
      <div className="flex-1 overflow-y-auto px-4 py-8 space-y-8">
        <div className="max-w-3xl mx-auto space-y-10">
            {messages.length === 0 && (
                <div className="flex flex-col items-center justify-center h-[50vh] text-center space-y-4">
                    <div className="w-16 h-16 bg-slate-900 rounded-2xl flex items-center justify-center mb-2 shadow-2xl border border-slate-800">
                        <Bot size={32} className="text-blue-500" />
                    </div>
                    <h2 className="text-2xl font-bold text-white">How can I help you today?</h2>
                    <p className="text-slate-500 max-w-sm">Ask me to execute build pipelines, audit cryptography, or validate network topologies.</p>
                </div>
            )}

            {messages.map((msg) => (
                <div key={msg.id} className={`flex gap-4 ${msg.role === 'user' ? 'justify-end' : 'justify-start'} group animate-in fade-in slide-in-from-bottom-2 duration-300`}>
                    {msg.role === 'assistant' && (
                        <div className="flex-shrink-0 w-8 h-8 rounded-lg bg-slate-900 border border-slate-800 flex items-center justify-center mt-1">
                            <div className="w-5 h-5 bg-blue-600 rounded-sm flex items-center justify-center text-[10px] font-bold text-white uppercase">
                                {msg.model ? msg.model.substring(0, 1) : 'G'}
                            </div>
                        </div>
                    )}

                    <div className={`max-w-[85%] flex flex-col gap-2 ${msg.role === 'user' ? 'items-end' : 'items-start'}`}>
                        <div className="flex items-center gap-2 px-1">
                            <span className="text-xs font-bold text-slate-200">
                                {msg.role === 'user' ? 'You' : (msg.model ? msg.model.split('/').pop() : 'Ghostlink')}
                            </span>
                            <span className="text-[10px] text-slate-500">{msg.timestamp}</span>
                        </div>

                        <div className={`px-4 py-3 rounded-2xl text-sm leading-relaxed ${
                            msg.role === 'user'
                                ? 'bg-slate-800 text-slate-100 rounded-tr-none'
                                : 'text-slate-200 bg-transparent'
                        }`}>
                            {msg.role === 'assistant' ? (
                              <div className="prose prose-invert prose-sm max-w-none break-words">
                                <ReactMarkdown remarkPlugins={[remarkGfm]}>{msg.content}</ReactMarkdown>
                              </div>
                            ) : (
                              <div className="whitespace-pre-wrap break-words">{msg.content}</div>
                            )}
                        </div>

                        {msg.role === 'assistant' && (
                            <div className="flex items-center gap-1 mt-1 opacity-0 group-hover:opacity-100 transition-opacity">
                                <button onClick={() => handleCopy(msg.content, msg.id)} className="p-1.5 rounded-lg hover:bg-slate-900 text-slate-500 hover:text-slate-300 transition" title="Copy">
                                  {copiedId === msg.id ? <Check size={14} className="text-green-400" /> : <Copy size={14} />}
                                </button>
                                <button className="p-1.5 rounded-lg hover:bg-slate-900 text-slate-500 hover:text-slate-300 transition" title="Good response">
                                    <ThumbsUp size={14} />
                                </button>
                                <button className="p-1.5 rounded-lg hover:bg-slate-900 text-slate-500 hover:text-slate-300 transition" title="Bad response">
                                    <ThumbsDown size={14} />
                                </button>
                                <button onClick={handleRegenerate} className="p-1.5 rounded-lg hover:bg-slate-900 text-slate-500 hover:text-slate-300 transition" title="Regenerate">
                                    <RotateCcw size={14} />
                                </button>
                                <button
                                  onClick={() => { try { speechSynthesis.speak(new SpeechSynthesisUtterance(msg.content)); } catch {} }}
                                  className="p-1.5 rounded-lg hover:bg-slate-900 text-slate-500 hover:text-slate-300 transition"
                                  title="Read aloud"
                                >
                                    <Volume2 size={14} />
                                </button>
                            </div>
                        )}
                    </div>

                    {msg.role === 'user' && (
                        <div className="flex-shrink-0 w-8 h-8 rounded-full bg-slate-800 border border-slate-700 flex items-center justify-center mt-1">
                            <User size={16} className="text-slate-400" />
                        </div>
                    )}
                </div>
            ))}

            {loading && (
                <div className="flex gap-4 justify-start animate-pulse">
                    <div className="flex-shrink-0 w-8 h-8 rounded-lg bg-slate-900 border border-slate-800 flex items-center justify-center mt-1">
                        <Loader size={14} className="text-blue-500 animate-spin" />
                    </div>
                    <div className="flex flex-col gap-2">
                        <div className="h-4 w-20 bg-slate-900 rounded"></div>
                        <div className="h-12 w-64 bg-slate-900/50 rounded-2xl"></div>
                    </div>
                </div>
            )}

            <div ref={messagesEndRef} />
        </div>
      </div>

      {/* Input Area */}
      <div className="px-4 pb-6 pt-2">
        <div className="max-w-3xl mx-auto relative group">
          {/* Active Tools Indicators */}
          <div className="flex flex-wrap gap-2 mb-2 px-1">
              {tools.filter(t => t.enabled).map(t => (
                  <div key={t.name} className="flex items-center gap-1.5 px-2 py-1 rounded-full bg-purple-500/10 border border-purple-500/20 text-[10px] text-purple-400 font-medium">
                      <Wand2 size={10} />
                      {t.name}
                      <button onClick={() => toggleTool(t.name)} className="hover:text-white"><X size={10} /></button>
                  </div>
              ))}
          </div>

          <div className="relative bg-slate-900 border border-slate-800 rounded-3xl p-2 shadow-2xl focus-within:border-slate-700 transition-all duration-300">
            <textarea
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={(e) => {
                  if (e.key === 'Enter' && !e.shiftKey) {
                      e.preventDefault();
                      handleSend();
                  }
              }}
              placeholder="Send a Message"
              className="w-full bg-transparent border-none text-slate-100 placeholder-slate-500 focus:ring-0 resize-none px-4 pt-3 pb-12 text-sm min-h-[56px] max-h-48"
              rows={1}
            />

            <div className="absolute left-3 bottom-3 flex items-center gap-1">
                <button
                    onClick={() => setShowTools(!showTools)}
                    className={`p-2 rounded-xl transition ${showTools ? 'bg-slate-800 text-blue-400' : 'text-slate-500 hover:text-slate-300 hover:bg-slate-800'}`}
                >
                    <Plus size={20} />
                </button>
                <button className="p-2 rounded-xl text-slate-500 hover:text-slate-300 hover:bg-slate-800 transition">
                    <LayoutGrid size={20} />
                </button>
            </div>

            <div className="absolute right-3 bottom-3 flex items-center gap-1">
                <button className="p-2 rounded-xl text-slate-500 hover:text-slate-300 hover:bg-slate-800 transition">
                    <Cloud size={18} />
                </button>
                <button className="p-2 rounded-xl text-slate-500 hover:text-slate-300 hover:bg-slate-800 transition">
                    <Mic size={18} />
                </button>
                <button
                    onClick={handleSend}
                    disabled={!input.trim() || loading}
                    className={`p-2 rounded-full transition shadow-lg ${
                        !input.trim() || loading
                            ? 'bg-slate-800 text-slate-600'
                            : 'bg-white text-black hover:bg-slate-200'
                    }`}
                >
                    {loading ? <Loader size={18} className="animate-spin" /> : <Send size={18} fill="currentColor" />}
                </button>
            </div>

            {/* Tool Selection Popup */}
            {showTools && (
                <div className="absolute left-0 bottom-16 w-64 bg-slate-900 border border-slate-800 rounded-2xl p-4 shadow-2xl z-20 animate-in fade-in slide-in-from-bottom-2">
                    <div className="flex items-center justify-between mb-3 px-1">
                        <h4 className="text-xs font-bold text-slate-400 uppercase tracking-wider">Capabilities</h4>
                        <button onClick={() => setShowTools(false)} className="text-slate-500 hover:text-white"><X size={14} /></button>
                    </div>
                    <div className="grid grid-cols-1 gap-1">
                        {tools.map(tool => (
                            <button
                                key={tool.name}
                                onClick={() => toggleTool(tool.name)}
                                className={`flex items-center justify-between w-full px-3 py-2 rounded-xl transition text-xs ${
                                    tool.enabled ? 'bg-blue-600/20 text-blue-400' : 'text-slate-400 hover:bg-slate-800'
                                }`}
                            >
                                <div className="flex items-center gap-2">
                                    <Wand2 size={14} className={tool.enabled ? 'text-blue-400' : 'text-slate-600'} />
                                    <span>{tool.name.replace('_', ' ')}</span>
                                </div>
                                {tool.enabled && <div className="w-1.5 h-1.5 bg-blue-400 rounded-full shadow-[0_0_8px_rgba(96,165,250,0.5)]"></div>}
                            </button>
                        ))}
                    </div>
                </div>
            )}
          </div>

          <div className="mt-2 text-center">
              <p className="text-[10px] text-slate-600">Ghostlink can make mistakes. Check important info.</p>
          </div>
        </div>
      </div>

      {error && (
          <div className="fixed bottom-24 right-4 max-w-sm p-4 bg-red-950 border border-red-900 rounded-2xl text-red-200 text-sm shadow-2xl animate-in fade-in slide-in-from-right-4 z-50">
              <div className="flex gap-3">
                  <X className="flex-shrink-0 text-red-500" size={18} />
                  <div>
                      <p className="font-bold">Execution Error</p>
                      <p className="text-red-400/80">{error}</p>
                  </div>
              </div>
          </div>
      )}
    </div>
  );
};
