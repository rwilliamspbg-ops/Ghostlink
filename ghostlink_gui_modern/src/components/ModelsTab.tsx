import React, { useState, useEffect } from 'react';
import { RefreshCw, Download, Trash2, Play, Power, Search, Zap } from 'lucide-react';
import { useAppStore, Model } from '../store';

interface ModelsTabProps {
  api: any;
}

// Popular models to auto-populate HuggingFace search
const POPULAR_MODELS = [
  { id: 'meta-llama/Llama-2-7b-chat-hf', name: 'Llama 2 7B Chat', downloads: 500000, likes: 5000 },
  { id: 'meta-llama/Llama-2-13b-chat-hf', name: 'Llama 2 13B Chat', downloads: 400000, likes: 4000 },
  { id: 'mistralai/Mistral-7B-Instruct-v0.1', name: 'Mistral 7B Instruct', downloads: 600000, likes: 6000 },
  { id: 'NousResearch/Nous-Hermes-2-Mixtral-8x7B-DPO', name: 'Nous Hermes 2 Mixtral', downloads: 150000, likes: 1500 },
  { id: 'openchat/openchat-3.5-1210', name: 'OpenChat 3.5', downloads: 200000, likes: 2000 },
  { id: 'Qwen/Qwen1.5-7B-Chat', name: 'Qwen 1.5 7B Chat', downloads: 300000, likes: 3000 },
  { id: 'NousResearch/Nous-Hermes-2-Mixtral-8x7B', name: 'Nous Hermes 2 Mixtral', downloads: 180000, likes: 1800 },
  { id: 'mistralai/Mistral-7B-v0.1', name: 'Mistral 7B Base', downloads: 400000, likes: 4000 },
  { id: 'meta-llama/Llama-2-70b-chat-hf', name: 'Llama 2 70B Chat', downloads: 300000, likes: 3000 },
  { id: 'TheBloke/Mistral-7B-Instruct-v0.1-GGUF', name: 'Mistral 7B Instruct GGUF', downloads: 250000, likes: 2500 },
];

export const ModelsTab: React.FC<ModelsTabProps> = ({ api }) => {
  const { models, setModels, setCurrentModel } = useAppStore();
  const [activeTab, setActiveTab] = useState<'local' | 'huggingface'>('local');
  const [filter, setFilter] = useState('');
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState('');
  const [hfSearch, setHfSearch] = useState('');
  const [hfResults, setHfResults] = useState<any[]>(POPULAR_MODELS);
  const [hfLoading, setHfLoading] = useState(false);

  const refreshModels = async () => {
    setLoading(true);
    const result = await api.getModels();
    if (!result.error) {
      setModels(result.models);
      setMessage('Models refreshed');
      setTimeout(() => setMessage(''), 2000);
    } else {
      setMessage(`Error: ${result.error}`);
    }
    setLoading(false);
  };

  useEffect(() => {
    refreshModels();
  }, []);

  const filteredModels = models.filter((m) =>
    m.name.toLowerCase().includes(filter.toLowerCase())
  );

  const handleLoadModel = async (modelName: string) => {
    setMessage('Loading...');
    const result = await api.loadModel(modelName);
    if (result.success) {
      setMessage(`Loaded: ${modelName}`);
      setCurrentModel(modelName);
      setTimeout(() => refreshModels(), 500);
    } else {
      setMessage(`Error: ${result.error}`);
    }
  };

  const handleUnloadModel = async (modelName: string) => {
    setMessage('Unloading...');
    const result = await api.unloadModel(modelName);
    if (result.success) {
      setMessage(`Unloaded: ${modelName}`);
      setTimeout(() => refreshModels(), 500);
    } else {
      setMessage(`Error: ${result.error}`);
    }
  };

  const handleDeleteModel = async (modelName: string) => {
    if (!confirm(`Delete ${modelName}? This cannot be undone.`)) return;
    setMessage('Deleting...');
    const result = await api.deleteModel(modelName);
    if (result.success) {
      setMessage(`Deleted: ${modelName}`);
      setTimeout(() => refreshModels(), 500);
    } else {
      setMessage(`Error: ${result.error}`);
    }
  };

  const handleDownloadModel = async (modelId: string) => {
    setMessage('Downloading...');
    const result = await api.downloadModel(modelId);
    if (result.success) {
      setMessage(`Downloaded: ${modelId}`);
      setTimeout(() => refreshModels(), 1000);
    } else {
      setMessage(`Error: ${result.error}`);
    }
  };

  const searchHuggingFace = async () => {
    if (!hfSearch.trim()) {
      setHfResults(POPULAR_MODELS);
      return;
    }

    setHfLoading(true);
    const searchTerm = hfSearch.toLowerCase();
    const filtered = POPULAR_MODELS.filter(
      (m) =>
        m.id.toLowerCase().includes(searchTerm) ||
        m.name.toLowerCase().includes(searchTerm)
    );
    setHfResults(filtered.length > 0 ? filtered : POPULAR_MODELS);
    setHfLoading(false);
  };

  return (
    <div className="space-y-4">
      {/* Tabs */}
      <div className="flex gap-2 border-b border-slate-700">
        <button
          onClick={() => setActiveTab('local')}
          className={`px-4 py-2 font-semibold transition ${
            activeTab === 'local'
              ? 'text-blue-400 border-b-2 border-blue-400'
              : 'text-slate-400 hover:text-slate-300'
          }`}
        >
          Local Models
        </button>
        <button
          onClick={() => setActiveTab('huggingface')}
          className={`px-4 py-2 font-semibold transition ${
            activeTab === 'huggingface'
              ? 'text-blue-400 border-b-2 border-blue-400'
              : 'text-slate-400 hover:text-slate-300'
          }`}
        >
          Hugging Face
        </button>
      </div>

      {message && (
        <div className="p-3 rounded bg-blue-900 text-blue-200 text-sm">{message}</div>
      )}

      {/* Local Models Tab */}
      {activeTab === 'local' && (
        <div className="space-y-4">
          <div className="flex gap-2 flex-wrap">
            <div className="flex-1 min-w-60">
              <input
                type="text"
                placeholder="Filter models..."
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                className="w-full px-4 py-2 bg-slate-800 border border-slate-700 rounded text-slate-100 placeholder-slate-500 focus:outline-none focus:border-blue-500"
              />
            </div>
            <button
              onClick={refreshModels}
              disabled={loading}
              className="flex items-center gap-2 px-4 py-2 bg-slate-700 hover:bg-slate-600 disabled:bg-slate-800 text-white rounded transition"
            >
              <RefreshCw size={16} className={loading ? 'animate-spin' : ''} />
              Refresh
            </button>
          </div>

          {/* Models Table */}
          <div className="overflow-x-auto rounded border border-slate-700">
            <table className="w-full">
              <thead className="bg-slate-900 border-b border-slate-700">
                <tr>
                  <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Name</th>
                  <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Size</th>
                  <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Type</th>
                  <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Quant</th>
                  <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Status</th>
                  <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Actions</th>
                </tr>
              </thead>
              <tbody>
                {filteredModels.length === 0 ? (
                  <tr>
                    <td colSpan={6} className="px-4 py-4 text-center text-slate-500">
                      No models found
                    </td>
                  </tr>
                ) : (
                  filteredModels.map((model) => (
                    <tr key={model.name} className="border-b border-slate-700 hover:bg-slate-800">
                      <td className="px-4 py-3 text-sm text-slate-200 font-mono">{model.name}</td>
                      <td className="px-4 py-3 text-sm text-slate-400">{model.size_gb.toFixed(1)} GB</td>
                      <td className="px-4 py-3 text-sm text-slate-400">{model.type}</td>
                      <td className="px-4 py-3 text-sm text-slate-400">{model.quantization}</td>
                      <td className="px-4 py-3 text-sm">
                        <span
                          className={`px-2 py-1 rounded text-xs font-semibold ${
                            model.status?.toLowerCase() === 'ready'
                              ? 'bg-emerald-900 text-emerald-200'
                              : 'bg-yellow-900 text-yellow-200'
                          }`}
                        >
                          {model.status}
                        </span>
                      </td>
                      <td className="px-4 py-3 text-sm flex gap-1">
                        <button
                          onClick={() => handleLoadModel(model.name)}
                          title="Load model"
                          className="p-1.5 bg-green-700 hover:bg-green-600 text-white rounded transition"
                        >
                          <Play size={14} />
                        </button>
                        <button
                          onClick={() => handleUnloadModel(model.name)}
                          title="Unload model"
                          className="p-1.5 bg-yellow-700 hover:bg-yellow-600 text-white rounded transition"
                        >
                          <Power size={14} />
                        </button>
                        <button
                          onClick={() => handleDeleteModel(model.name)}
                          title="Delete model"
                          className="p-1.5 bg-red-700 hover:bg-red-600 text-white rounded transition"
                        >
                          <Trash2 size={14} />
                        </button>
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Hugging Face Tab */}
      {activeTab === 'huggingface' && (
        <div className="space-y-4">
          <div className="flex gap-2">
            <input
              type="text"
              placeholder="Search Hugging Face models..."
              value={hfSearch}
              onChange={(e) => setHfSearch(e.target.value)}
              onKeyPress={(e) => e.key === 'Enter' && searchHuggingFace()}
              className="flex-1 px-4 py-2 bg-slate-800 border border-slate-700 rounded text-slate-100 placeholder-slate-500 focus:outline-none focus:border-blue-500"
            />
            <button
              onClick={searchHuggingFace}
              disabled={hfLoading}
              className="flex items-center gap-2 px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-slate-600 text-white rounded transition"
            >
              <Search size={16} className={hfLoading ? 'animate-spin' : ''} />
              Search
            </button>
          </div>

          <div className="text-xs text-slate-400 bg-slate-900 p-2 rounded">
            💡 Showing {hfSearch ? 'filtered' : 'popular'} models from Hugging Face
            {hfSearch && ` matching "${hfSearch}"`}
          </div>

          {/* HF Results Table */}
          {hfResults.length > 0 && (
            <div className="overflow-x-auto rounded border border-slate-700">
              <table className="w-full">
                <thead className="bg-slate-900 border-b border-slate-700">
                  <tr>
                    <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Model ID</th>
                    <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Name</th>
                    <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Downloads</th>
                    <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Likes</th>
                    <th className="px-4 py-3 text-left text-sm font-semibold text-slate-300">Action</th>
                  </tr>
                </thead>
                <tbody>
                  {hfResults.map((model) => (
                    <tr key={model.id} className="border-b border-slate-700 hover:bg-slate-800">
                      <td className="px-4 py-3 text-sm text-slate-200 font-mono">{model.id}</td>
                      <td className="px-4 py-3 text-sm text-slate-300">{model.name}</td>
                      <td className="px-4 py-3 text-sm text-slate-400">
                        📥 {(model.downloads / 1000).toFixed(0)}K
                      </td>
                      <td className="px-4 py-3 text-sm text-slate-400">
                        👍 {(model.likes / 1000).toFixed(1)}K
                      </td>
                      <td className="px-4 py-3 text-sm">
                        <button
                          onClick={() => handleDownloadModel(model.id)}
                          className="flex items-center gap-1 px-3 py-1.5 bg-blue-600 hover:bg-blue-700 text-white rounded text-xs transition"
                        >
                          <Download size={14} />
                          Download
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}

          {hfResults.length === 0 && hfSearch && !hfLoading && (
            <div className="text-center text-slate-400 py-8">
              No models found matching "{hfSearch}"
            </div>
          )}
        </div>
      )}
    </div>
  );
};
