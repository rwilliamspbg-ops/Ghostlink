import React, { useState, useEffect, useCallback } from 'react';
import {
  Play,
  Trash2,
  RefreshCw,
  Download,
  Search,
  Database,
  Cpu,
  Layers,
  CheckCircle2,
  AlertCircle,
  Loader,
  Power,
  ChevronRight,
  ArrowUpRight,
  Copy,
} from 'lucide-react';
import { useAppStore } from '../store';
import { GhostlinkAPI } from '../api';

const POPULAR_MODELS = [
  { id: 'llama3.2:3b', name: 'Llama 3.2 3B Instruct', downloads: 2500000, likes: 120000 },
  { id: 'llama3.2:1b', name: 'Llama 3.2 1B Instruct', downloads: 1800000, likes: 90000 },
  { id: 'gemma2:2b', name: 'Gemma 2 2B Instruct', downloads: 1500000, likes: 80000 },
  { id: 'qwen2.5:3b', name: 'Qwen 2.5 3B Instruct', downloads: 3200000, likes: 150000 },
  { id: 'mistral:7b', name: 'Mistral 7B Instruct', downloads: 4100000, likes: 200000 },
  { id: 'phi3:mini', name: 'Phi-3 Mini', downloads: 1200000, likes: 60000 },
  { id: 'tinyllama', name: 'TinyLlama 1.1B', downloads: 900000, likes: 45000 },
  { id: 'codegemma:2b', name: 'CodeGemma 2B', downloads: 800000, likes: 40000 },
];

export const ModelsTab: React.FC<{ api: GhostlinkAPI }> = ({ api }) => {
  const { models, currentModel, setModels, setCurrentModel } = useAppStore();
  const [activeTab, setActiveTab] = useState<'local' | 'huggingface'>('local');
  const [loading, setLoading] = useState(false);
  const [hfSearch, setHfSearch] = useState('');
  const [message, setMessage] = useState<string | null>(null);
  const [pendingActions, setPendingActions] = useState<Record<string, string>>({});
  const [hfResults, setHfResults] = useState<{ id: string; name: string; downloads: number; likes: number }[]>(POPULAR_MODELS);
  const [downloadProgress, setDownloadProgress] = useState<Record<string, number>>({});
  const [ollamaModels, setOllamaModels] = useState<any[]>([]);
  const [showModelfile, setShowModelfile] = useState<string | null>(null);

  const searchHF = useCallback(async (query: string) => {
    try {
      const result = await api.searchHuggingFace(query || 'llama');
      if (result.models && result.models.length > 0) setHfResults(result.models);
    } catch {
      // silent
    }
  }, [api]);

  useEffect(() => {
    if (activeTab === 'huggingface' && hfResults.length === 0) {
      searchHF('llama');
    }
  }, [activeTab, searchHF, hfResults.length]);

  useEffect(() => {
    const timer = setTimeout(() => {
      if (hfSearch.length > 0) searchHF(hfSearch);
    }, 300);
    return () => clearTimeout(timer);
  }, [hfSearch, searchHF]);

  const filteredHfResults = hfResults.filter(model =>
    model.name.toLowerCase().includes(hfSearch.toLowerCase()) ||
    model.id.toLowerCase().includes(hfSearch.toLowerCase())
  );

  const refreshModels = async () => {
    setLoading(true);
    try {
      const result = await api.getModels();
      if (result.models) {
        setModels(result.models);
        if (result.current_model) setCurrentModel(result.current_model);
      }
      // Also fetch Ollama models
      const ollamaResult = await api.getOllamaModels();
      if (ollamaResult.models) {
        setOllamaModels(ollamaResult.models);
      }
    } catch (e) {
      console.error('Failed to refresh models:', e);
    }
    setLoading(false);
  };

  const handleLoadModel = async (name: string) => {
    setPendingActions(prev => ({ ...prev, [name]: 'loading' }));
    setMessage(`Setting ${name} as current model...`);
    try {
      setCurrentModel(name);
      setMessage(`Current model set to ${name}`);
    } finally {
      setPendingActions(prev => {
        const newState = { ...prev };
        delete newState[name];
        return newState;
      });
    }
  };

  const handleDeleteModel = async (name: string) => {
    if (!window.confirm(`Are you sure you want to delete the model "${name}" from Ollama? This cannot be undone.`)) {
      return;
    }
    setPendingActions(prev => ({ ...prev, [name]: 'deleting' }));
    setMessage(`Deleting ${name}...`);
    try {
      const result = await api.deleteOllamaModel(name);
      if (result.success) {
        setMessage(`Deleted ${name}`);
        if (name === currentModel) {
          setCurrentModel('none');
        }
        refreshModels();
      } else {
        setMessage(`Error: ${result.error}`);
      }
    } finally {
      setPendingActions(prev => {
        const newState = { ...prev };
        delete newState[name];
        return newState;
      });
    }
  };

  const handlePullModel = async (id: string) => {
    setPendingActions(prev => ({ ...prev, [id]: 'downloading' }));
    setMessage(`Pulling ${id}...`);
    setDownloadProgress(prev => ({ ...prev, [id]: 0 }));

    try {
      const result = await api.pullOllamaModelStream(id, (progress: any) => {
        if (progress.completed && progress.total) {
          const pct = progress.completed / progress.total;
          setDownloadProgress(prev => ({ ...prev, [id]: pct }));
        }
        if (progress.status === 'success') {
          setDownloadProgress(prev => ({ ...prev, [id]: 1 }));
        }
      });

      if (result.success) {
        setMessage(`Pulled ${id}`);
        refreshModels();
      } else {
        setMessage(`Error: ${result.error}`);
      }
    } catch (e: any) {
      setMessage(`Error: ${e.message}`);
    } finally {
      setPendingActions(prev => {
        const newState = { ...prev };
        delete newState[id];
        return newState;
      });
    }
  };

  const handleDownloadHFModel = async (id: string) => {
    setPendingActions(prev => ({ ...prev, [id]: 'downloading' }));
    setMessage(`Downloading ${id}...`);
    setDownloadProgress(prev => ({ ...prev, [id]: 0 }));

    const dlResult = await api.downloadModel(id);
    if (!dlResult.success) {
      setMessage(`Error: ${dlResult.error}`);
      setPendingActions(prev => {
        const newState = { ...prev };
        delete newState[id];
        return newState;
      });
      return;
    }

    refreshModels();

    let pollsRemaining = 300;
    const poll = async () => {
      while (pollsRemaining > 0) {
        pollsRemaining--;
        await new Promise(resolve => setTimeout(resolve, 2000));
        const result = await api.getDownloadProgress(id);
        const progress = result.progress ?? 0;
        const status = result.status ?? 'unknown';
        setDownloadProgress(prev => ({ ...prev, [id]: progress }));

        if (status === 'completed') {
          setDownloadProgress(prev => ({ ...prev, [id]: 1 }));
          setMessage(`Downloaded ${id}`);
          refreshModels();
          setPendingActions(prev => {
            const newState = { ...prev };
            delete newState[id];
            return newState;
          });
          return;
        }

        if (status === 'failed') {
          setMessage(`Download failed: ${id}`);
          refreshModels();
          setPendingActions(prev => {
            const newState = { ...prev };
            delete newState[id];
            return newState;
          });
          return;
        }
      }
      setMessage(`Download timed out: ${id}`);
      setPendingActions(prev => {
        const newState = { ...prev };
        delete newState[id];
        return newState;
      });
    };

    poll();
  };

  const handleShowModelfile = async (name: string) => {
    try {
      const result = await api.showOllamaModel(name);
      if (result.modelfile) {
        setShowModelfile(result.modelfile);
      }
    } catch (e) {
      setMessage(`Error fetching modelfile: ${e}`);
    }
  };

  const copyModelfile = () => {
    if (showModelfile) {
      navigator.clipboard.writeText(showModelfile);
      setMessage('Modelfile copied to clipboard');
    }
  };

  return (
    <div className="flex flex-col h-full bg-slate-950">
      <div className="flex items-center justify-between px-6 py-4 border-b border-slate-900 sticky top-0 bg-slate-950/50 backdrop-blur-md z-10">
        <div className="flex items-center gap-4">
          <button
            onClick={() => setActiveTab('local')}
            className={`px-3 py-1.5 rounded-lg text-sm font-bold transition ${
              activeTab === 'local' ? 'bg-blue-600 text-white shadow-lg shadow-blue-500/20' : 'text-slate-400 hover:bg-slate-900'
            }`}
          >
            Ollama Models
          </button>
          <button
            onClick={() => setActiveTab('huggingface')}
            className={`px-3 py-1.5 rounded-lg text-sm font-bold transition ${
              activeTab === 'huggingface' ? 'bg-blue-600 text-white shadow-lg shadow-blue-500/20' : 'text-slate-400 hover:bg-slate-900'
            }`}
          >
            Hugging Face
          </button>
        </div>
        <div className="flex items-center gap-4">
          {activeTab === 'huggingface' && (
            <div className="relative">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-slate-400" />
              <input
                type="text"
                value={hfSearch}
                onChange={(e) => setHfSearch(e.target.value)}
                placeholder="Search models..."
                name="hf-search"
                id="hf-search"
                className="pl-8 pr-4 py-2 bg-slate-800 border border-slate-700 rounded-lg text-slate-200 placeholder-slate-400 focus:outline-none focus:ring-2 focus:ring-blue-500"
              />
            </div>
          )}
          <button
            onClick={refreshModels}
            className="flex items-center gap-2 px-4 py-2 bg-slate-800 hover:bg-blue-600 text-white rounded-xl text-xs font-bold transition group-hover:shadow-lg group-hover:shadow-blue-500/20"
          >
            <RefreshCw size={16} /> Refresh
          </button>
        </div>
      </div>
      <div className="flex-1 overflow-y-auto p-6">
        {message && (
          <div className="mb-4 px-4 py-2 bg-slate-800 border-l-4 border-blue-500 text-sm">
            {message}
          </div>
        )}
        {activeTab === 'local' ? (
          <div className="space-y-4">
            <div className="flex items-center justify-between px-4 py-3 bg-slate-800 rounded-lg">
              <div className="flex items-center gap-3">
                <Database size={20} className="text-blue-400" />
                <h2 className="text-lg font-bold text-white">Ollama Models</h2>
              </div>
              <div className="text-sm text-slate-400">
                {ollamaModels.length} models available
              </div>
            </div>
            <div className="space-y-2">
              {ollamaModels.length === 0 ? (
                <div className="text-center py-12 text-slate-500">
                  <Database size={48} className="mx-auto mb-4 text-slate-700" />
                  <p>No Ollama models found. Pull a model from the list below or run <code>{'ollama pull <model>'}</code></p>
                </div>
              ) : (
                ollamaModels.map((model: any) => (
                  <div key={model.name} className="flex flex-col px-4 py-3 bg-slate-800/50 rounded-lg border border-slate-800 hover:border-slate-700 transition">
                    <div className="flex items-center justify-between">
                      <div className="flex items-center gap-3">
                        <div className="flex items-center gap-2">
                          <Cpu className="h-5 w-5 text-slate-400" />
                          <div>
                            <div className="font-semibold text-white">{model.name}</div>
                            <div className="text-xs text-slate-400">
                              {(model.size / (1024 * 1024 * 1024)).toFixed(2)} GB • {model.details?.family || 'Unknown'} • {model.details?.quantization_level || 'Unknown'}
                            </div>
                          </div>
                        </div>
                        <div className="flex items-center gap-2">
                          <CheckCircle2 className="h-4 w-4 text-green-500" />
                          <span className="text-xs text-slate-400">Ready</span>
                        </div>
                      </div>
                      <div className="flex items-center gap-2">
                        {model.name === currentModel ? (
                          <span className="px-3 py-1 bg-blue-600 text-white text-xs font-medium rounded-full">
                            Active
                          </span>
                        ) : (
                          <button
                            onClick={() => handleLoadModel(model.name)}
                            disabled={pendingActions[model.name] === 'loading' || loading}
                            className="flex items-center gap-2 px-3 py-1 bg-slate-800 hover:bg-blue-600 text-xs font-medium rounded-full transition"
                          >
                            {pendingActions[model.name] === 'loading' ? (
                              <Loader size={16} className="mr-1" />
                            ) : (
                              <Play size={16} />
                            )}
                            Use
                          </button>
                        )}
                        <button
                          onClick={() => handleShowModelfile(model.name)}
                          className="p-1 hover:bg-slate-700 rounded-lg transition text-slate-400 hover:text-white"
                          title="View Modelfile"
                        >
                          <Copy size={16} />
                        </button>
                        <button
                          onClick={() => handleDeleteModel(model.name)}
                          disabled={pendingActions[model.name] === 'deleting' || loading}
                          className="p-1 hover:bg-slate-700 rounded-lg transition text-slate-400 hover:text-red-400"
                          title="Delete from Ollama"
                        >
                          {pendingActions[model.name] === 'deleting' ? (
                            <Loader size={16} className="mr-1" />
                          ) : (
                            <Trash2 size={16} />
                          )}
                        </button>
                      </div>
                    </div>
                  </div>
                ))
              )}
            </div>
            <div className="mt-6 p-4 bg-slate-800/50 rounded-lg border border-slate-800">
              <h3 className="text-lg font-bold text-white mb-3">Popular Ollama Models</h3>
              <div className="grid grid-cols-2 md:grid-cols-4 gap-2">
                {POPULAR_MODELS.map((m) => {
                  const isInstalled = ollamaModels.some((om: any) => om.name === m.id);
                  const isPending = pendingActions[m.id] === 'downloading';
                  return (
                    <button
                      key={m.id}
                      onClick={() => handlePullModel(m.id)}
                      disabled={isInstalled || isPending || loading}
                      className={`flex flex-col items-center p-3 rounded-xl text-left transition ${
                        isInstalled
                          ? 'bg-green-900/30 border border-green-700 cursor-default'
                          : isPending
                          ? 'bg-blue-900/30 border border-blue-700 cursor-wait'
                          : 'bg-slate-800 hover:bg-blue-900/30 border border-slate-700'
                      }`}
                    >
                      <div className="flex items-center gap-2 mb-2">
                        <span className="font-bold text-slate-200 truncate w-full">{m.name}</span>
                        {isInstalled && <CheckCircle2 className="h-4 w-4 text-green-500 flex-shrink-0" />}
                      </div>
                      <div className="text-xs text-slate-400 truncate w-full font-mono">{m.id}</div>
                      {isPending && downloadProgress[m.id] !== undefined && (
                        <div className="mt-2 w-full bg-slate-700 rounded-full h-1.5">
                          <div
                            className="bg-blue-500 h-1.5 rounded-full transition-all duration-500"
                            style={{ width: `${Math.round(downloadProgress[m.id] * 100)}%` }}
                          />
                        </div>
                      )}
                      {!isInstalled && !isPending && (
                        <div className="mt-2 flex items-center gap-1 text-xs text-slate-400">
                          <Download size={12} /> Pull
                        </div>
                      )}
                    </button>
                  );
                })}
              </div>
            </div>
          </div>
        ) : (
          <div className="space-y-4">
            <div className="flex items-center justify-between px-4 py-3 bg-slate-800 rounded-lg">
              <div className="flex items-center gap-3">
                <Layers size={20} className="text-blue-400" />
                <h2 className="text-lg font-bold text-white">Hugging Face Models</h2>
              </div>
            </div>
            <div className="space-y-2">
              {filteredHfResults.map((m) => (
                <div key={m.id} className="flex items-center justify-between px-4 py-3 hover:bg-slate-800/50 rounded-lg transition">
                  <div className="flex items-center gap-3 min-w-0">
                    <div className="flex items-center justify-center text-orange-500 bg-orange-500/10 h-10 w-10 rounded-lg shrink-0">
                      <ChevronRight size={14} />
                    </div>
                    <div className="min-w-0">
                      <div className="font-bold text-slate-200 truncate">{m.name}</div>
                      <div className="text-[10px] text-slate-500 font-mono truncate">{m.id}</div>
                    </div>
                  </div>
                  <div className="flex items-center gap-4 shrink-0">
                    <div className="text-xs text-slate-400">
                      <span className="mr-3">📥 {(m.downloads / 1000).toFixed(0)}K</span>
                      <span>👍 {(m.likes / 1000).toFixed(1)}K</span>
                    </div>
                    <button
                      onClick={() => handleDownloadHFModel(m.id)}
                      disabled={pendingActions[m.id] === 'downloading' || loading}
                      className="inline-flex items-center gap-2 px-4 py-2 bg-slate-800 hover:bg-blue-600 text-white rounded-xl text-xs font-bold transition group-hover:shadow-lg group-hover:shadow-blue-500/20"
                    >
                      {pendingActions[m.id] === 'downloading' ? (
                        <>
                          <Loader size={14} className="mr-2" />
                          {downloadProgress[m.id] !== undefined ? `${Math.round(downloadProgress[m.id] * 100)}%` : '...'}
                        </>
                      ) : (
                        <Download size={14} />
                      )}
                      {pendingActions[m.id] === 'downloading' ? '' : 'Download'}
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Modelfile Modal */}
      {showModelfile && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-slate-900 border border-slate-800 rounded-2xl max-w-2xl w-full mx-4 max-h-[80vh] overflow-hidden">
            <div className="flex items-center justify-between p-4 border-b border-slate-800">
              <h3 className="text-lg font-bold text-white">Modelfile</h3>
              <button onClick={() => setShowModelfile(null)} className="text-slate-400 hover:text-white">
                <Power size={20} />
              </button>
            </div>
            <div className="p-4 overflow-y-auto max-h-[60vh]">
              <pre className="bg-slate-950 p-4 rounded-lg text-sm text-slate-300 overflow-x-auto font-mono whitespace-pre-wrap">
                {showModelfile}
              </pre>
            </div>
            <div className="flex justify-end gap-2 p-4 border-t border-slate-800">
              <button onClick={copyModelfile} className="px-4 py-2 bg-blue-600 text-white rounded-lg font-medium hover:bg-blue-700">
                Copy to Clipboard
              </button>
              <button onClick={() => setShowModelfile(null)} className="px-4 py-2 bg-slate-800 text-white rounded-lg font-medium hover:bg-slate-700">
                Close
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};