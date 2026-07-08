import React, { useState } from 'react';
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

const POPULAR_MODELS = [
  { id: 'meta-llama/Llama-3-8B-Instruct', name: 'Llama 3 8B', downloads: 1200000, likes: 45000 },
  { id: 'mistralai/Mistral-7B-Instruct-v0.2', name: 'Mistral 7B v0.2', downloads: 950000, likes: 38000 },
  { id: 'google/gemma-7b-it', name: 'Gemma 7B', downloads: 800000, likes: 25000 },
  { id: 'microsoft/phi-3-mini-4k-instruct', name: 'Phi-3 Mini', downloads: 600000, likes: 18000 },
  { id: 'stabilityai/stablelm-zephyr-3b', name: 'StableLM Zephyr', downloads: 300000, likes: 12000 },
];

export const ModelsTab: React.FC<{ api: GhostlinkAPI }> = ({ api }) => {
  const { models, currentModel, setModels, setCurrentModel } = useAppStore();
  const [activeTab, setActiveTab] = useState<'local' | 'huggingface'>('local');
  const [loading, setLoading] = useState(false);
  const [hfSearch, setHfSearch] = useState('');
  const [hfResults, setHfResults] = useState(POPULAR_MODELS);
  const [message, setMessage] = useState<string | null>(null);

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
    setMessage(`Loading ${name}...`);
    const result = await api.loadModel(name);
    if (result.success) {
      setMessage(`Loaded ${name}`);
      refreshModels();
    } else {
      setMessage(`Error: ${result.error}`);
    }
  };

  const handleDownloadModel = async (id: string) => {
    setMessage(`Downloading ${id}...`);
    const result = await api.downloadModel(id);
    if (result.success) {
        setMessage(`Downloaded ${id}`);
        refreshModels();
    } else {
        setMessage(`Error: ${result.error}`);
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
        <button
          onClick={refreshModels}
          className="p-2 rounded-lg hover:bg-slate-900 text-slate-400 hover:text-white transition"
        >
          <RefreshCw size={18} className={loading ? 'animate-spin' : ''} />
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        <div className="max-w-5xl mx-auto space-y-6">
          {message && (
            <div className="flex items-center gap-3 p-4 bg-blue-500/10 border border-blue-500/20 rounded-2xl text-blue-400 text-sm animate-in fade-in slide-in-from-top-2">
              <Loader size={16} className="animate-spin" />
              <span>{message}</span>
            </div>
          )}

          {activeTab === 'local' ? (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              {models.map((model) => (
                <div key={model.name} className={`p-5 rounded-2xl border transition-all duration-300 ${
                  model.status === 'Loaded'
                    ? 'bg-blue-600/5 border-blue-500/30 ring-1 ring-blue-500/20'
                    : 'bg-slate-900/50 border-slate-800 hover:border-slate-700'
                }`}>
                  <div className="flex items-start justify-between mb-4">
                    <div className="flex items-center gap-3">
                      <div className={`p-2.5 rounded-xl ${
                        model.status === 'Loaded' ? 'bg-blue-500 text-white' : 'bg-slate-800 text-slate-400'
                      }`}>
                        <Database size={20} />
                      </div>
                      <div>
                        <h3 className="font-bold text-slate-100 truncate max-w-[200px]">{model.name.split('/').pop()}</h3>
                        <p className="text-[10px] text-slate-500 font-mono">{model.name}</p>
                      </div>
                    </div>
                    <div className={`px-2 py-1 rounded-md text-[10px] font-bold uppercase tracking-wider ${
                      model.status === 'Loaded' ? 'bg-green-500/20 text-green-400' : 'bg-slate-800 text-slate-500'
                    }`}>
                      {model.status}
                    </div>
                  </div>

                  <div className="grid grid-cols-3 gap-2 mb-5">
                    <div className="bg-slate-950/50 p-2 rounded-xl border border-slate-800/50">
                        <p className="text-[10px] text-slate-500 mb-0.5">Size</p>
                        <p className="text-xs font-bold">{model.size_gb.toFixed(1)} GB</p>
                    </div>
                    <div className="bg-slate-950/50 p-2 rounded-xl border border-slate-800/50">
                        <p className="text-[10px] text-slate-500 mb-0.5">Quant</p>
                        <p className="text-xs font-bold uppercase">{model.quantization}</p>
                    </div>
                    <div className="bg-slate-950/50 p-2 rounded-xl border border-slate-800/50">
                        <p className="text-[10px] text-slate-500 mb-0.5">Type</p>
                        <p className="text-xs font-bold uppercase">{model.type}</p>
                    </div>
                  </div>

                  <div className="flex gap-2">
                    {model.status !== 'Loaded' ? (
                        <button
                          onClick={() => handleLoadModel(model.name)}
                          className="flex-1 flex items-center justify-center gap-2 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-xl text-xs font-bold transition shadow-lg shadow-blue-500/20"
                        >
                          <Play size={14} fill="currentColor" /> Load Model
                        </button>
                    ) : (
                        <button className="flex-1 flex items-center justify-center gap-2 py-2 bg-slate-800 text-slate-400 rounded-xl text-xs font-bold cursor-default">
                          <CheckCircle2 size={14} /> Currently Active
                        </button>
                    )}
                    <button className="p-2 bg-slate-800 hover:bg-red-500/10 text-slate-500 hover:text-red-400 rounded-xl transition border border-transparent hover:border-red-500/20">
                      <Trash2 size={16} />
                    </button>
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div className="space-y-4">
              <div className="relative group">
                <Search className="absolute left-4 top-1/2 -translate-y-1/2 text-slate-500 group-focus-within:text-blue-400 transition" size={18} />
                <input
                  type="text"
                  placeholder="Search Hugging Face..."
                  value={hfSearch}
                  onChange={(e) => setHfSearch(e.target.value)}
                  className="w-full bg-slate-900 border border-slate-800 rounded-2xl py-3 pl-12 pr-4 text-sm focus:outline-none focus:border-slate-700 focus:ring-1 focus:ring-blue-500/20 transition shadow-2xl"
                />
              </div>

              <div className="bg-slate-900/50 border border-slate-800 rounded-3xl overflow-hidden shadow-2xl">
                <table className="w-full text-left text-sm">
                  <thead className="bg-slate-900 text-slate-400 font-bold text-[10px] uppercase tracking-wider">
                    <tr>
                      <th className="px-6 py-4">Model Repository</th>
                      <th className="px-6 py-4">Popularity</th>
                      <th className="px-6 py-4 text-right">Action</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-slate-800/50">
                    {hfResults.map((m) => (
                      <tr key={m.id} className="hover:bg-slate-800/30 transition-colors group">
                        <td className="px-6 py-4">
                          <div className="flex items-center gap-3">
                            <div className="w-8 h-8 rounded-lg bg-orange-500/10 flex items-center justify-center text-orange-500">
                                <ChevronRight size={14} />
                            </div>
                            <div>
                                <div className="font-bold text-slate-200 group-hover:text-white transition">{m.name}</div>
                                <div className="text-[10px] text-slate-500 font-mono">{m.id}</div>
                            </div>
                          </div>
                        </td>
                        <td className="px-6 py-4 text-xs text-slate-400">
                            <span className="mr-3">📥 {(m.downloads / 1000).toFixed(0)}K</span>
                            <span>👍 {(m.likes / 1000).toFixed(1)}K</span>
                        </td>
                        <td className="px-6 py-4 text-right">
                          <button
                            onClick={() => handleDownloadModel(m.id)}
                            className="inline-flex items-center gap-2 px-4 py-2 bg-slate-800 hover:bg-blue-600 text-white rounded-xl text-xs font-bold transition group-hover:shadow-lg group-hover:shadow-blue-500/20"
                          >
                            <Download size={14} /> Download
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
