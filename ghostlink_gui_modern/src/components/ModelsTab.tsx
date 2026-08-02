import React, { useState, useEffect, useCallback, useRef } from 'react';
import {
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
  ChevronRight,
  Copy,
  Check,
  X,
} from 'lucide-react';
import { useAppStore, DownloadProgressEntry } from '../store';
import { GhostlinkAPI } from '../api';
import { useInferenceEngines } from '../hooks/useInferenceEngines';
import { EmptyState, LoadingState } from './StatusViews';

function formatBytes(bytes?: number): string {
  if (!bytes || bytes <= 0) return '';
  const gb = bytes / (1024 * 1024 * 1024);
  if (gb >= 1) return `${gb.toFixed(2)} GB`;
  return `${(bytes / (1024 * 1024)).toFixed(0)} MB`;
}

// A real, byte-accurate progress label like "1.52 GB / 4.30 GB" when the
// backend has reported a total size, falling back to a bare percentage
// before the first size-probing pass completes.
function progressLabel(entry: DownloadProgressEntry | undefined): string {
  if (!entry) return '...';
  const pct = `${Math.round(entry.progress * 100)}%`;
  if (entry.totalBytes) {
    return `${formatBytes(entry.bytesDownloaded)} / ${formatBytes(entry.totalBytes)} (${pct})`;
  }
  return pct;
}

function formatAge(seconds: number): string {
  if (seconds < 60) return 'just now';
  const mins = Math.floor(seconds / 60);
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

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
  const {
    currentModel, setModels, setCurrentModel,
    pendingModelActions: pendingActions, setPendingModelActions: setPendingActions,
    downloadProgress, setDownloadProgress,
    addToast,
  } = useAppStore();
  const [activeTab, setActiveTab] = useState<'local' | 'huggingface' | 'recommended'>('local');
  const [loading, setLoading] = useState(false);
  const [hfSearch, setHfSearch] = useState('');
  const [message, setMessage] = useState<string | null>(null);
  const [hfResults, setHfResults] = useState<{ id: string; name: string; downloads: number; likes: number }[]>([]);
  const [hfLoading, setHfLoading] = useState(false);
  const [ollamaModels, setOllamaModels] = useState<any[]>([]);
  const [showModelfile, setShowModelfile] = useState<string | null>(null);
  const [recommendedModels, setRecommendedModels] = useState<any[]>([]);
  const [recommendedLoading, setRecommendedLoading] = useState(false);
  const [detectedRuntime, setDetectedRuntime] = useState<string>('cpu');
  const [availableMemoryGb, setAvailableMemoryGb] = useState<number>(0);
  const { currentEngine, selectedEngine } = useInferenceEngines(api);

  const engineLabel = selectedEngine?.label || 'Ollama';
  const canUnload = selectedEngine?.capabilities.model_unload ?? true;
  const canManageRemoteCatalog = currentEngine === 'ollama';
  const canShowModelDefinition = currentEngine === 'ollama';
  const canDownloadLocalModels = currentEngine !== 'vllm';
  const [partialDownloads, setPartialDownloads] = useState<{ filename: string; size_bytes: number; age_secs: number }[]>([]);
  const [discardingPartial, setDiscardingPartial] = useState<string | null>(null);

  // `message` used to render as a permanent inline banner with no way to
  // dismiss it — forwarding to the shared toast queue instead gives every
  // one of these status strings (in-progress and completed/error alike)
  // a lifecycle that actually ends.
  useEffect(() => {
    if (!message) return;
    const type = /error|failed|timed out/i.test(message)
      ? 'error'
      : message.endsWith('...')
        ? 'info'
        : 'success';
    addToast({ type, message });
    setMessage(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [message]);

  const searchHF = useCallback(async (query: string) => {
    setHfLoading(true);
    try {
      const result = await api.searchHuggingFace(query || 'llama');
      if (result.models && result.models.length > 0) setHfResults(result.models);
    } catch {
      // silent
    } finally {
      setHfLoading(false);
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

  const fetchRecommendations = useCallback(async () => {
    setRecommendedLoading(true);
    try {
      const runtimeResult = await api.getRuntimes();
      if (!runtimeResult.error && runtimeResult.available_runtimes && runtimeResult.available_runtimes.length > 0) {
        const primary = runtimeResult.available_runtimes.find((r: any) => r.is_primary || r.is_available);
        const runtime = primary || runtimeResult.available_runtimes[0];
        const runtimeName = runtime.runtime || runtime.name || 'cpu';
        const memoryGb = runtime.memory_gb || runtime.vram_gb || runtime.system_memory_gb || 8;
        
        setDetectedRuntime(runtimeName);
        setAvailableMemoryGb(memoryGb);

        const recResult = await api.getModelRecommendations(runtimeName, memoryGb);
        if (!recResult.error && recResult.recommended_models) {
          setRecommendedModels(recResult.recommended_models);
        }
      }
    } catch (e) {
      console.error('Failed to fetch recommendations:', e);
    }
    setRecommendedLoading(false);
  }, [api]);

  useEffect(() => {
    fetchRecommendations();
  }, [fetchRecommendations]);

  // hfResults already reflects the live server-side search for hfSearch (see searchHF),
  // so no further client-side filtering is applied here.
  const filteredHfResults = hfResults;

  const refreshPartialDownloads = useCallback(async () => {
    const result = await api.listPartialDownloads();
    if (!result.error) setPartialDownloads(result.partials);
  }, [api]);

  const refreshRequestIdRef = useRef(0);

  const refreshModels = useCallback(async () => {
    // Each call captures its own engine-specific requestId. If a newer
    // refreshModels() call starts (e.g. currentEngine flips from the
    // 'ollama' default to the real engine right after mount) before this
    // one's slower awaits (getOllamaModels can take seconds when Ollama
    // isn't running) resolve, the stale call must not clobber the fresher
    // state — otherwise the model list flashes correct then reverts to
    // empty once the superseded 'ollama' branch finally lands.
    const requestId = ++refreshRequestIdRef.current;
    setLoading(true);
    try {
      const result = await api.getModels();
      if (requestId !== refreshRequestIdRef.current) return;
      if (result.models) {
        setModels(result.models);
        if (result.current_model) setCurrentModel(result.current_model);
        if (currentEngine === 'vllm') {
          const inventory = await api.getVllmModels();
          if (requestId !== refreshRequestIdRef.current) return;
          setOllamaModels(
            (inventory.models || []).map((m: any) => ({
              name: m.name,
              size: 0,
              details: {
                family: m.source || 'remote',
                quantization_level: 'server-managed',
              },
              status: m.status || 'Ready',
              source: m.source || 'vllm',
              usable: true,
            }))
          );
        } else if (currentEngine === 'ollama') {
          const inventory = await api.getOllamaModels();
          if (requestId !== refreshRequestIdRef.current) return;
          if (!inventory.error && inventory.models) {
            setOllamaModels(inventory.models);
          } else {
            setOllamaModels(
              result.models.map((m: any) => ({
                name: m.name,
                size: Math.max(0, Number(m.size_gb || 0)) * 1024 * 1024 * 1024,
                details: {
                  family: m.type || 'unknown',
                  quantization_level: m.quantization || 'unknown',
                },
                status: m.status || 'unknown',
                usable: !!m.usable,
              }))
            );
          }
        } else {
          setOllamaModels(
            result.models.map((m: any) => ({
              name: m.name,
              size: Math.max(0, Number(m.size_gb || 0)) * 1024 * 1024 * 1024,
              details: {
                family: m.type || 'unknown',
                quantization_level: m.quantization || 'unknown',
              },
              status: m.status || 'unknown',
              source: 'native',
              usable: !!m.usable,
            }))
          );
        }
      }
    } catch (e) {
      console.error('Failed to refresh models:', e);
    }
    if (requestId !== refreshRequestIdRef.current) return;
    setLoading(false);
    refreshPartialDownloads();
  }, [api, currentEngine, refreshPartialDownloads, setCurrentModel, setModels]);

  useEffect(() => {
    refreshModels();
  }, [refreshModels]);

  const handleDiscardPartial = async (filename: string) => {
    setDiscardingPartial(filename);
    const result = await api.discardPartialDownload(filename);
    if (result.success) {
      setPartialDownloads((prev) => prev.filter((p) => p.filename !== filename));
      addToast({ type: 'success', message: `Discarded ${filename}` });
    } else {
      addToast({ type: 'error', message: result.error || `Failed to discard ${filename}` });
    }
    setDiscardingPartial(null);
  };

  const handleSetModel = async (name: string) => {
    setPendingActions(prev => ({ ...prev, [name]: 'setting' }));
    setMessage(`Setting current model to ${name}...`);
    try {
      const result = await api.loadModel(name);
      if (result.success) {
        setCurrentModel(name);
        setMessage(`Current model set to ${name}`);
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
    if (!canManageRemoteCatalog) {
      setMessage(`${engineLabel} models are server-managed and cannot be deleted from this UI.`);
      return;
    }
    if (!window.confirm(`Are you sure you want to delete the model "${name}" from Ollama? This cannot be undone.`)) {
      return;
    }
    setPendingActions(prev => ({ ...prev, [name]: 'deleting' }));
    setMessage(`Deleting ${name}...`);
    try {
      const result = await api.deleteModel(name);
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

  const handleUnloadModel = async (name: string) => {
    if (!canUnload) {
      setMessage(`${engineLabel} does not support model unload from this UI.`);
      return;
    }
    setPendingActions(prev => ({ ...prev, [name]: 'unloading' }));
    setMessage(`Unloading ${name}...`);
    try {
      const result = await api.unloadModel(name);
      if (result.success) {
        if (name === currentModel) {
          setCurrentModel('none');
        }
        setMessage(`Unloaded ${name}`);
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
    if (!canManageRemoteCatalog) {
      setMessage(`${engineLabel} inventories are managed by the remote server. Change the server model set outside Ghostlink.`);
      return;
    }
    setPendingActions(prev => ({ ...prev, [id]: 'downloading' }));
    setMessage(`Pulling ${id}...`);

    try {
      const result = await api.pullOllamaModel(id);

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
    if (!canDownloadLocalModels) {
      setMessage('Download is disabled while vLLM is selected because the remote server owns its model inventory.');
      return;
    }
    setPendingActions(prev => ({ ...prev, [id]: 'downloading' }));
    setMessage(`Downloading ${id}...`);
    setDownloadProgress(prev => ({ ...prev, [id]: { progress: 0 } }));

    // The backend now starts the transfer in a detached background task and
    // responds immediately with {status: "started"} — it no longer blocks
    // this request for the entire multi-GB download, so this resolves in
    // milliseconds regardless of model size. Polling below is what tracks
    // real progress from here; it isn't gated on waiting for the request above.
    const dlResult = await api.downloadModel(id);
    if (!dlResult.success) {
      setMessage(`Error: ${dlResult.error}`);
      setPendingActions(prev => {
        const newState = { ...prev };
        delete newState[id];
        return newState;
      });
      setDownloadProgress(prev => {
        const newState = { ...prev };
        delete newState[id];
        return newState;
      });
      return;
    }

    refreshModels();

    // A large GGUF over a slow connection can legitimately take a long time
    // now that it's no longer cut off by a client-side request timeout —
    // bound the poll loop generously (3 hours at 2s/poll) rather than at the
    // old 10-minute cap.
    let pollsRemaining = 5400;
    const poll = async () => {
      while (pollsRemaining > 0) {
        pollsRemaining--;
        await new Promise(resolve => setTimeout(resolve, 2000));
        const result = await api.getDownloadProgress(id);
        const progress = result.progress ?? 0;
        const status = result.status ?? 'unknown';
        setDownloadProgress(prev => ({
          ...prev,
          [id]: { progress, bytesDownloaded: result.bytesDownloaded, totalBytes: result.totalBytes },
        }));

        if (status === 'completed') {
          setMessage(`Downloaded ${id}`);
          refreshModels();
          setPendingActions(prev => {
            const newState = { ...prev };
            delete newState[id];
            return newState;
          });
          setDownloadProgress(prev => {
            const newState = { ...prev };
            delete newState[id];
            return newState;
          });
          return;
        }

        if (status === 'failed') {
          setMessage(`Download failed: ${id}${result.error ? ` (${result.error})` : ''}`);
          refreshModels();
          setPendingActions(prev => {
            const newState = { ...prev };
            delete newState[id];
            return newState;
          });
          setDownloadProgress(prev => {
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
    if (!canShowModelDefinition) {
      setMessage(`${engineLabel} does not expose Ollama modelfiles in this UI.`);
      return;
    }
    try {
      const result = await api.showOllamaModel(name);
      if (result.info?.modelfile) {
        setShowModelfile(result.info.modelfile);
      } else if (result.error) {
        setMessage(`Error: ${result.error}`);
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

  const modelfileCloseRef = React.useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!showModelfile) return;
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setShowModelfile(null);
    };
    document.addEventListener('keydown', handleKey);
    modelfileCloseRef.current?.focus();
    return () => document.removeEventListener('keydown', handleKey);
  }, [showModelfile]);

  return (
    <div className="flex flex-col h-full bg-slate-950">
      <div className="flex items-center justify-between px-6 py-4 border-b border-slate-900 sticky top-0 bg-slate-950/50 backdrop-blur-md z-10">
        <div className="flex items-center gap-4" role="tablist" aria-label="Model source">
          <button
            role="tab"
            aria-selected={activeTab === 'local'}
            onClick={() => setActiveTab('local')}
            className={`px-3 py-1.5 rounded-lg text-sm font-bold transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none ${
              activeTab === 'local' ? 'bg-blue-600 text-white shadow-lg shadow-blue-500/20' : 'text-slate-400 hover:bg-slate-900'
            }`}
          >
            My Models
          </button>
          <button
            role="tab"
            aria-selected={activeTab === 'recommended'}
            onClick={() => setActiveTab('recommended')}
            className={`px-3 py-1.5 rounded-lg text-sm font-bold transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none ${
              activeTab === 'recommended' ? 'bg-blue-600 text-white shadow-lg shadow-blue-500/20' : 'text-slate-400 hover:bg-slate-900'
            }`}
          >
            Recommended
          </button>
          <button
            role="tab"
            aria-selected={activeTab === 'huggingface'}
            onClick={() => setActiveTab('huggingface')}
            className={`px-3 py-1.5 rounded-lg text-sm font-bold transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none ${
              activeTab === 'huggingface' ? 'bg-blue-600 text-white shadow-lg shadow-blue-500/20' : 'text-slate-400 hover:bg-slate-900'
            }`}
          >
            Hugging Face
          </button>
        </div>
        <div className="flex items-center gap-4">
          {activeTab === 'huggingface' && (
            <div className="relative">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-slate-400" aria-hidden="true" />
              <label htmlFor="hf-search" className="sr-only">Search Hugging Face models</label>
              <input
                type="text"
                value={hfSearch}
                onChange={(e) => setHfSearch(e.target.value)}
                placeholder="Search models..."
                name="hf-search"
                id="hf-search"
                className="pl-8 pr-4 py-2 bg-slate-800 border border-slate-700 rounded-lg text-slate-200 placeholder-slate-400 focus:outline-none focus:ring-2 focus:ring-blue-500 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
              />
            </div>
          )}
          <button
            onClick={refreshModels}
            className="flex items-center gap-2 px-4 py-2 bg-slate-800 hover:bg-blue-600 text-white rounded-xl text-xs font-bold transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none group-hover:shadow-lg group-hover:shadow-blue-500/20"
          >
            <RefreshCw size={16} aria-hidden="true" /> Refresh
          </button>
        </div>
      </div>
      <div className="flex-1 overflow-y-auto p-6">
        {activeTab === 'local' ? (
          <div className="space-y-4">
            <div className="flex items-center justify-between px-4 py-3 bg-slate-800 rounded-lg">
              <div className="flex items-center gap-3">
                <Database size={20} className="text-blue-400" />
                <h2 className="text-lg font-bold text-white">{currentEngine === 'ollama' ? 'My Models' : `${engineLabel} Models`}</h2>
              </div>
              <div className="text-sm text-slate-400">
                {ollamaModels.length} models available
              </div>
            </div>

            {currentEngine === 'vllm' && (
              <div className="px-4 py-3 bg-blue-500/10 border border-blue-500/20 rounded-lg text-sm text-slate-300">
                vLLM inventory is read from the remote server. Loading a model selects one that is already being served; download, pull, and delete actions are disabled here.
              </div>
            )}

            <div className="space-y-2">
              {ollamaModels.length === 0 ? (
                <EmptyState
                  icon={Database}
                  title="No models found"
                  description={
                    currentEngine === 'vllm'
                      ? 'No vLLM models were reported by the remote server.'
                      : currentEngine === 'ollama'
                        ? <>Drop a .gguf into <code>models/</code>, or pull one from the list below.</>
                        : 'No local native models found. Download a model below or add a GGUF file locally.'
                  }
                />
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
                              {model.size > 0 ? `${(model.size / (1024 * 1024 * 1024)).toFixed(2)} GB` : 'Size unknown'} • {model.details?.family || 'Unknown'} • {model.details?.quantization_level || 'Unknown'}
                            </div>
                          </div>
                        </div>
                        <div className="flex items-center gap-2">
                          {currentEngine === 'vllm' ? (
                            <>
                              <CheckCircle2 className="h-4 w-4 text-green-500" />
                              <span className="text-xs text-slate-400">{model.status || 'Ready'}</span>
                            </>
                          ) : model.usable ? (
                            <>
                              <CheckCircle2 className="h-4 w-4 text-green-500" />
                              <span className="text-xs text-slate-400">Ready</span>
                            </>
                          ) : (
                            <>
                              <AlertCircle className="h-4 w-4 text-amber-500" />
                              <span className="text-xs text-amber-500">Not downloaded</span>
                            </>
                          )}
                        </div>
                      </div>
                      <div className="flex items-center gap-2">
                        {model.name === currentModel ? (
                          <div className="flex items-center gap-2">
                            <span className="px-3 py-1 bg-blue-600 text-white text-xs font-medium rounded-full">
                              Active
                            </span>
                            {canUnload && (
                              <button
                                onClick={() => handleUnloadModel(model.name)}
                                disabled={pendingActions[model.name] === 'unloading' || loading}
                                className="flex items-center gap-2 px-3 py-1 bg-slate-800 hover:bg-slate-700 text-xs font-medium rounded-full transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                              >
                                {pendingActions[model.name] === 'unloading' ? (
                                  <Loader size={16} className="mr-1" />
                                ) : (
                                  <X size={16} />
                                )}
                                Unload
                              </button>
                            )}
                          </div>
                        ) : (
                          <button
                            onClick={() => handleSetModel(model.name)}
                            disabled={pendingActions[model.name] === 'setting' || loading}
                            title={model.usable ? undefined : 'Not downloaded yet — selecting will show an error until it is'}
                            className="flex items-center gap-2 px-3 py-1 bg-slate-800 hover:bg-blue-600 text-xs font-medium rounded-full transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                          >
                            {pendingActions[model.name] === 'setting' ? (
                              <Loader size={16} className="mr-1" />
                            ) : (
                              <Check size={16} />
                            )}
                            Use
                          </button>
                        )}
                        {canShowModelDefinition && (
                          <button
                            onClick={() => handleShowModelfile(model.name)}
                            className="p-1 hover:bg-slate-700 rounded-lg transition text-slate-400 hover:text-white focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                            title="View Modelfile"
                            aria-label={`View Modelfile for ${model.name}`}
                          >
                            <Copy size={16} aria-hidden="true" />
                          </button>
                        )}
                        {canManageRemoteCatalog && (
                          <button
                            onClick={() => handleDeleteModel(model.name)}
                            disabled={pendingActions[model.name] === 'deleting' || loading}
                            className="p-1 hover:bg-slate-700 rounded-lg transition text-slate-400 hover:text-red-400 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                            title="Delete from Ollama"
                            aria-label={`Delete ${model.name} from Ollama`}
                          >
                            {pendingActions[model.name] === 'deleting' ? (
                              <Loader size={16} className="mr-1" aria-hidden="true" />
                            ) : (
                              <Trash2 size={16} aria-hidden="true" />
                            )}
                          </button>
                        )}
                      </div>
                    </div>
                  </div>
                ))
              )}
            </div>

            {partialDownloads.length > 0 && (
              <div className="mt-6 p-4 bg-amber-500/5 rounded-lg border border-amber-500/20">
                <div className="flex items-center gap-2 mb-2">
                  <AlertCircle size={16} className="text-amber-500" aria-hidden="true" />
                  <h3 className="text-sm font-bold text-amber-400">Interrupted Downloads</h3>
                </div>
                <p className="text-xs text-slate-500 mb-3">
                  These downloads didn't finish and aren't used by anything — discard to reclaim disk space, or re-download from the Hugging Face tab.
                </p>
                <div className="space-y-2">
                  {partialDownloads.map((p) => (
                    <div key={p.filename} className="flex items-center justify-between px-3 py-2 bg-slate-900/50 rounded-lg border border-slate-800">
                      <div className="min-w-0">
                        <div className="font-mono text-xs text-slate-300 truncate">{p.filename}</div>
                        <div className="text-[10px] text-slate-500">{formatBytes(p.size_bytes)} partial • {formatAge(p.age_secs)}</div>
                      </div>
                      <button
                        onClick={() => handleDiscardPartial(p.filename)}
                        disabled={discardingPartial === p.filename}
                        className="flex items-center gap-1.5 px-3 py-1.5 bg-slate-800 hover:bg-red-600 text-slate-300 hover:text-white text-xs font-bold rounded-lg transition disabled:opacity-50 shrink-0 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                        aria-label={`Discard interrupted download ${p.filename}`}
                      >
                        {discardingPartial === p.filename ? (
                          <Loader size={12} className="animate-spin" aria-hidden="true" />
                        ) : (
                          <Trash2 size={12} aria-hidden="true" />
                        )}
                        Discard
                      </button>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {canManageRemoteCatalog && (
              <div className="mt-6 p-4 bg-slate-800/50 rounded-lg border border-slate-800">
                <h3 className="text-lg font-bold text-white mb-3">Popular Ollama Models</h3>
                <div className="grid grid-cols-2 md:grid-cols-4 gap-2">
                  {POPULAR_MODELS.map((m) => {
                    const normalize = (s: string) => s.toLowerCase().replace(/[^a-z0-9]/g, '');
                    const normId = normalize(m.id.split(':')[0]);
                    const isInstalled = ollamaModels.some(
                      (om: any) => om.usable && (om.name === m.id || normalize(om.name).includes(normId))
                    );
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
                          <div className="mt-2 w-full">
                            <div className="w-full bg-slate-700 rounded-full h-1.5">
                              <div
                                className="bg-blue-500 h-1.5 rounded-full transition-all duration-500"
                                style={{ width: `${Math.round(downloadProgress[m.id].progress * 100)}%` }}
                              />
                            </div>
                            <div className="mt-1 text-[10px] text-slate-500 font-mono">
                              {progressLabel(downloadProgress[m.id])}
                            </div>
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
            )}
          </div>
        ) : activeTab === 'recommended' ? (
          <div className="space-y-4">
            <div className="flex items-center justify-between px-4 py-3 bg-slate-800 rounded-lg">
              <div className="flex items-center gap-3">
                <Cpu size={20} className="text-blue-400" />
                <h2 className="text-lg font-bold text-white">Recommended for Your Hardware</h2>
              </div>
              <div className="text-sm text-slate-400">
                Runtime: <span className="font-mono text-blue-400 capitalize">{detectedRuntime}</span> • {availableMemoryGb > 0 ? `${availableMemoryGb.toFixed(1)} GB available` : 'Unknown memory'}
              </div>
            </div>
            {recommendedLoading ? (
              <LoadingState label="Analyzing hardware & finding best models..." />
            ) : recommendedModels.length === 0 ? (
              <EmptyState
                icon={Cpu}
                title="No model recommendations available"
                description="Try refreshing or check if runtime detection is working."
              />
            ) : (
              <div className="space-y-2">
                {recommendedModels.map((model: any) => (
                  <div key={model.name} className="flex items-center justify-between px-4 py-3 hover:bg-slate-800/50 rounded-lg transition">
                    <div className="flex items-center gap-3 min-w-0">
                      <div className="flex items-center justify-center text-blue-500 bg-blue-500/10 h-10 w-10 rounded-lg shrink-0">
                        <CheckCircle2 size={14} />
                      </div>
                      <div className="min-w-0">
                        <div className="font-bold text-slate-200 truncate">{model.name}</div>
                        <div className="text-[10px] text-slate-500 font-mono truncate">{model.parameters} • {model.size_gb} GB • {model.quality_tier} • {model.inference_speed}</div>
                      </div>
                    </div>
                    <div className="flex items-center gap-4 shrink-0">
                      <div className="text-xs text-slate-400">
                        {model.reason}
                      </div>
                      <button
                        onClick={() => handlePullModel(model.name)}
                        disabled={pendingActions[model.name] === 'downloading' || loading || !canManageRemoteCatalog}
                        className="inline-flex items-center gap-2 px-4 py-2 bg-slate-800 hover:bg-blue-600 text-white rounded-xl text-xs font-bold transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none group-hover:shadow-lg group-hover:shadow-blue-500/20"
                      >
                        {pendingActions[model.name] === 'downloading' ? (
                          <>
                            <Loader size={14} className="mr-2" />
                            {progressLabel(downloadProgress[model.name])}
                          </>
                        ) : (
                          <Download size={14} />
                        )}
                        {pendingActions[model.name] === 'downloading' ? '' : canManageRemoteCatalog ? 'Pull' : 'Server managed'}
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
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
              {hfLoading && filteredHfResults.length === 0 ? (
                <LoadingState label="Searching Hugging Face..." />
              ) : filteredHfResults.length === 0 ? (
                <EmptyState icon={Layers} title="No models found" />
              ) : (
              filteredHfResults.map((m) => (
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
                      className="inline-flex items-center gap-2 px-4 py-2 bg-slate-800 hover:bg-blue-600 text-white rounded-xl text-xs font-bold transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none group-hover:shadow-lg group-hover:shadow-blue-500/20"
                      disabled={pendingActions[m.id] === 'downloading' || loading || !canDownloadLocalModels}
                    >
                      {pendingActions[m.id] === 'downloading' ? (
                        <>
                          <Loader size={14} className="mr-2" />
                          {progressLabel(downloadProgress[m.id])}
                        </>
                      ) : (
                        <Download size={14} />
                      )}
                      {pendingActions[m.id] === 'downloading' ? '' : canDownloadLocalModels ? 'Download' : 'Server managed'}
                    </button>
                  </div>
                </div>
              ))
              )}
            </div>
          </div>
        )}
      </div>

      {/* Modelfile Modal */}
      {showModelfile && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onClick={() => setShowModelfile(null)}>
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="modelfile-modal-title"
            onClick={(e) => e.stopPropagation()}
            className="bg-slate-900 border border-slate-800 rounded-2xl max-w-2xl w-full mx-4 max-h-[80vh] overflow-hidden"
          >
            <div className="flex items-center justify-between p-4 border-b border-slate-800">
              <h3 id="modelfile-modal-title" className="text-lg font-bold text-white">Modelfile</h3>
              <button ref={modelfileCloseRef} onClick={() => setShowModelfile(null)} className="text-slate-400 hover:text-white focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none" aria-label="Close">
                <X size={20} aria-hidden="true" />
              </button>
            </div>
            <div className="p-4 overflow-y-auto max-h-[60vh]">
              <pre className="bg-slate-950 p-4 rounded-lg text-sm text-slate-300 overflow-x-auto font-mono whitespace-pre-wrap">
                {showModelfile}
              </pre>
            </div>
            <div className="flex justify-end gap-2 p-4 border-t border-slate-800">
              <button onClick={copyModelfile} className="px-4 py-2 bg-blue-600 text-white rounded-lg font-medium hover:bg-blue-700 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none">
                Copy to Clipboard
              </button>
              <button onClick={() => setShowModelfile(null)} className="px-4 py-2 bg-slate-800 text-white rounded-lg font-medium hover:bg-slate-700 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none">
                Close
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};