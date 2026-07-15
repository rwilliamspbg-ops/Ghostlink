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
} from 'lucide-react';
import { useAppStore } from '../store';
import { GhostlinkAPI } from '../api';

const POPULAR_MODELS: { id: string; name: string; downloads: number; likes: number }[] = [];

export const ModelsTab: React.FC<{ api: GhostlinkAPI }> = ({ api }) => {
  const { models, currentModel, setModels, setCurrentModel } = useAppStore();
  const [activeTab, setActiveTab] = useState<'local' | 'huggingface'>('local');
  const [loading, setLoading] = useState(false);
  const [hfSearch, setHfSearch] = useState('');
  const [message, setMessage] = useState<string | null>(null);
  const [pendingActions, setPendingActions] = useState<Record<string, string>>({});
  const [hfResults, setHfResults] = useState<{ id: string; name: string; downloads: number; likes: number }[]>([]);
  const [hfSearching, setHfSearching] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState<Record<string, number>>({});

  const searchHF = useCallback(async (query: string) => {
    setHfSearching(true);
    try {
      const result = await api.searchHuggingFace(query || 'llama');
      if (result.models) setHfResults(result.models);
    } catch {
      // silent
    }
    setHfSearching(false);
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
    const result = await api.getModels();
    setLoading(false);
    if (result.models) {
      setModels(result.models);
      if (result.current_model) setCurrentModel(result.current_model);
    }
  };

  const handleLoadModel = async (name: string) => {
    setPendingActions(prev => ({ ...prev, [name]: 'loading' }));
    setMessage(`Loading ${name}...`);
    try {
      const result = await api.loadModel(name);
      if (result.success) {
        setMessage(`Loaded ${name}`);
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

  const handleUnloadModel = async (name: string) => {
    setPendingActions(prev => ({ ...prev, [name]: 'unloading' }));
    setMessage(`Unloading ${name}...`);
    try {
      const result = await api.unloadModel(name);
      if (result.success) {
        setMessage(`Unloaded ${name}`);
        // Immediately reset currentModel if this was the current model
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

  const handleDeleteModel = async (name: string) => {
    if (!window.confirm(`Are you sure you want to delete the model "${name}"? This cannot be undone.`)) {
      return;
    }
    setPendingActions(prev => ({ ...prev, [name]: 'deleting' }));
    setMessage(`Deleting ${name}...`);
    try {
      const result = await api.deleteModel(name);
      if (result.success) {
        setMessage(`Deleted ${name}`);
        // Immediately reset currentModel if this was the current model
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

  const handleDownloadModel = async (id: string) => {
    setPendingActions(prev => ({ ...prev, [id]: 'downloading' }));
    setMessage(`Downloading ${id}...`);
    setDownloadProgress(prev => ({ ...prev, [id]: 0 }));

    // Poll progress every 2 seconds
    const progressInterval = setInterval(async () => {
      const result = await api.getDownloadProgress(id);
      if (result.progress !== undefined) {
        setDownloadProgress(prev => ({ ...prev, [id]: result.progress }));
      }
      if (result.status === 'completed' || result.status === 'failed') {
        clearInterval(progressInterval);
      }
    }, 2000);

    try {
      const result = await api.downloadModel(id);
      if (result.success) {
        setDownloadProgress(prev => ({ ...prev, [id]: 1 }));
        setMessage(`Downloaded ${id}`);
        refreshModels();
      } else {
        setMessage(`Error: ${result.error}`);
      }
    } finally {
      clearInterval(progressInterval);
      setPendingActions(prev => {
        const newState = { ...prev };
        delete newState[id];
        return newState;
      });
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
            Library
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
                <h2 className="text-lg font-bold text-white">Local Models</h2>
              </div>
              <div className="text-sm text-slate-400">
                {models.length} models loaded
              </div>
            </div>
            <div className="space-y-2">
              {models.map((model) => (
                <div key={model.name} className="flex flex-col px-4 py-3 bg-slate-800/50 rounded-lg border border-slate-800 hover:border-slate-700 transition">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-3">
                      <div className="flex items-center gap-2">
                        <Cpu className="h-5 w-5 text-slate-400" />
                        <div>
                          <div className="font-semibold text-white">{model.name}</div>
                          <div className="text-xs text-slate-400">{model.type}</div>
                        </div>
                      </div>
                      <div className="flex items-center gap-2">
                        {model.usable ? (
                          <CheckCircle2 className="h-4 w-4 text-green-500" />
                        ) : model.status === 'Downloading' ? (
                          <Loader className="h-4 w-4 text-blue-500 animate-spin" />
                        ) : (
                          <AlertCircle className="h-4 w-4 text-red-500" />
                        )}
                        <span className="text-xs text-slate-400">{model.status}</span>
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      {model.name === currentModel && pendingActions[model.name] !== 'unloading' && pendingActions[model.name] !== 'deleting' ? (
                        <button
                          onClick={() => handleUnloadModel(model.name)}
                          disabled={pendingActions[model.name] === 'unloading' || pendingActions[model.name] === 'deleting'}
                          className="flex items-center gap-2 px-3 py-1 bg-slate-800 hover:bg-blue-600 text-xs font-medium rounded-full transition"
                        >
                          {pendingActions[model.name] === 'unloading' ? (
                            <Loader size={16} className="mr-1" />
                          ) : (
                            <Power size={16} />
                          )}
                          Unload
                        </button>
                      ) : model.status !== 'Downloading' ? (
                        <button
                          onClick={() => handleLoadModel(model.name)}
                          disabled={pendingActions[model.name] === 'loading' || pendingActions[model.name] === 'unloading' || pendingActions[model.name] === 'deleting' || loading}
                          className="flex items-center gap-2 px-3 py-1 bg-slate-800 hover:bg-blue-600 text-xs font-medium rounded-full transition"
                        >
                          {pendingActions[model.name] === 'loading' ? (
                            <Loader size={16} className="mr-1" />
                          ) : (
                            <Play size={16} />
                          )}
                          Load
                        </button>
                      ) : null}
                      {model.status !== 'Downloading' && (
                        <button
                          onClick={() => handleDeleteModel(model.name)}
                          disabled={pendingActions[model.name] === 'deleting' || pendingActions[model.name] === 'loading' || pendingActions[model.name] === 'unloading' || loading}
                          className="p-1 hover:bg-slate-700 rounded-lg transition text-slate-400 hover:text-white"
                        >
                          {pendingActions[model.name] === 'deleting' ? (
                            <Loader size={16} className="mr-1" />
                          ) : (
                            <Trash2 size={16} />
                          )}
                        </button>
                      )}
                    </div>
                  </div>
                  {model.status === 'Downloading' && downloadProgress[model.name] !== undefined && (
                    <div className="mt-2 w-full bg-slate-700 rounded-full h-1.5">
                      <div
                        className="bg-blue-500 h-1.5 rounded-full transition-all duration-500"
                        style={{ width: `${Math.round(downloadProgress[model.name] * 100)}%` }}
                      />
                    </div>
                  )}
                </div>
              ))}
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
                      onClick={() => handleDownloadModel(m.id)}
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
    </div>
  );
};