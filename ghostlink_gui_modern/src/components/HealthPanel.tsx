import React, { useState, useEffect, useCallback } from 'react';
import { GhostlinkAPI } from '../api';
import { useAppStore } from '../store';
import { resolveApiBase } from '../config';
import { AlertTriangle, CheckCircle2, RefreshCw, Key, Layers, Server, ShieldAlert, Cpu } from 'lucide-react';

interface HealthPanelProps {
  api: GhostlinkAPI;
  onNavigateToTab?: (tab: string) => void;
}

export const HealthPanel: React.FC<HealthPanelProps> = ({ api, onNavigateToTab }) => {
  const { addToast } = useAppStore();

  const [probing, setProbing] = useState(false);
  const [controlPlaneStatus, setControlPlaneStatus] = useState<'healthy' | 'unauthorized' | 'down' | 'checking'>('checking');
  const [internalApiStatus, setInternalApiStatus] = useState<'healthy' | 'unauthorized' | 'down' | 'checking'>('checking');
  const [inferenceBackendStatus, setInferenceBackendStatus] = useState<'healthy' | 'down' | 'checking'>('checking');
  const [loadedModel, setLoadedModel] = useState<string | null>(null);

  const [inputApiKey, setInputApiKey] = useState('');
  const currentApiBase = resolveApiBase();

  const runProbes = useCallback(async () => {
    setProbing(true);
    setControlPlaneStatus('checking');
    setInternalApiStatus('checking');
    setInferenceBackendStatus('checking');

    // Probe 1: Control-plane / API health endpoint via standard client
    try {
      const healthRes = await api.getHealth();
      if (healthRes.success) {
        setControlPlaneStatus('healthy');
      } else if (healthRes.error && /401|unauthorized|bearer/i.test(healthRes.error)) {
        setControlPlaneStatus('unauthorized');
      } else {
        setControlPlaneStatus('down');
      }
    } catch (err: any) {
      if (err?.response?.status === 401) {
        setControlPlaneStatus('unauthorized');
      } else {
        setControlPlaneStatus('down');
      }
    }

    // Probe 2: Internal API status route (/api/models)
    try {
      const modelsRes = await api.getModels();
      if (modelsRes.models || modelsRes.current_model) {
        setInternalApiStatus('healthy');
        setLoadedModel(modelsRes.current_model || null);
      } else if (modelsRes.error && /401|unauthorized/i.test(modelsRes.error)) {
        setInternalApiStatus('unauthorized');
      } else {
        setInternalApiStatus('healthy');
      }
    } catch (err: any) {
      if (err?.response?.status === 401) {
        setInternalApiStatus('unauthorized');
      } else {
        setInternalApiStatus('down');
      }
    }

    // Probe 3: Inference engines / backend probe
    try {
      const enginesRes = await api.getInferenceEngines();
      if (enginesRes.engines || enginesRes.current) {
        setInferenceBackendStatus('healthy');
      } else {
        setInferenceBackendStatus('down');
      }
    } catch {
      setInferenceBackendStatus('down');
    }

    setProbing(false);
  }, [api]);

  useEffect(() => {
    runProbes();
  }, [runProbes]);

  useEffect(() => {
    const handleRetry = () => {
      runProbes();
    };
    window.addEventListener('retry-health-check', handleRetry);
    return () => window.removeEventListener('retry-health-check', handleRetry);
  }, [runProbes]);

  const handleApplyApiKey = (e: React.FormEvent) => {
    e.preventDefault();
    if (!inputApiKey.trim()) return;
    api.setApiKey(inputApiKey.trim());
    localStorage.setItem('ghostlink_api_key', inputApiKey.trim());
    addToast({ type: 'success', message: 'API key updated. Re-testing health...' });
    runProbes();
  };

  const isAllHealthy =
    controlPlaneStatus === 'healthy' &&
    internalApiStatus === 'healthy' &&
    inferenceBackendStatus === 'healthy';

  return (
    <div className="p-6 bg-slate-900 border border-slate-800 rounded-2xl space-y-6 max-w-4xl mx-auto shadow-xl">
      <div className="flex items-center justify-between border-b border-slate-800 pb-4">
        <div className="flex items-center gap-3">
          <div className={`p-2 rounded-xl ${isAllHealthy ? 'bg-green-500/10 text-green-400' : 'bg-amber-500/10 text-amber-400'}`}>
            {isAllHealthy ? <CheckCircle2 size={24} /> : <AlertTriangle size={24} />}
          </div>
          <div>
            <h2 className="text-xl font-bold text-white">System Health & Recovery</h2>
            <p className="text-xs text-slate-400">Diagnostic status across control-plane gateway, API routes, and inference backend</p>
          </div>
        </div>
        <button
          onClick={runProbes}
          disabled={probing}
          className="flex items-center gap-2 px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-300 hover:text-white rounded-lg text-xs font-bold transition disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
          title="Re-run health probes"
        >
          <RefreshCw size={14} className={probing ? 'animate-spin' : ''} />
          {probing ? 'Probing...' : 'Re-probe'}
        </button>
      </div>

      {/* Probes Grid */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        {/* Control-plane probe */}
        <div className="p-4 bg-slate-950/60 rounded-xl border border-slate-800 flex flex-col justify-between">
          <div className="flex items-center justify-between mb-2">
            <div className="flex items-center gap-2 text-slate-300 font-bold text-sm">
              <Server size={16} className="text-blue-400" /> Control-plane Gateway
            </div>
            <span className="text-[10px] font-mono text-slate-500">:8000</span>
          </div>
          <div className="mt-2">
            {controlPlaneStatus === 'checking' ? (
              <span className="text-xs text-slate-400 font-mono">Checking...</span>
            ) : controlPlaneStatus === 'healthy' ? (
              <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs font-bold bg-green-500/10 text-green-400 border border-green-500/20">
                <CheckCircle2 size={12} /> Reachable
              </span>
            ) : controlPlaneStatus === 'unauthorized' ? (
              <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs font-bold bg-amber-500/10 text-amber-400 border border-amber-500/20">
                <ShieldAlert size={12} /> 401 Unauthorized
              </span>
            ) : (
              <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs font-bold bg-red-500/10 text-red-400 border border-red-500/20">
                <AlertTriangle size={12} /> Process Offline / Unreachable
              </span>
            )}
          </div>
        </div>

        {/* Internal ghost-link API probe */}
        <div className="p-4 bg-slate-950/60 rounded-xl border border-slate-800 flex flex-col justify-between">
          <div className="flex items-center justify-between mb-2">
            <div className="flex items-center gap-2 text-slate-300 font-bold text-sm">
              <Cpu size={16} className="text-purple-400" /> Ghostlink API Engine
            </div>
            <span className="text-[10px] font-mono text-slate-500">:8003</span>
          </div>
          <div className="mt-2">
            {internalApiStatus === 'checking' ? (
              <span className="text-xs text-slate-400 font-mono">Checking...</span>
            ) : internalApiStatus === 'healthy' ? (
              <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs font-bold bg-green-500/10 text-green-400 border border-green-500/20">
                <CheckCircle2 size={12} /> Active
              </span>
            ) : internalApiStatus === 'unauthorized' ? (
              <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs font-bold bg-amber-500/10 text-amber-400 border border-amber-500/20">
                <ShieldAlert size={12} /> 401 Unauthorized
              </span>
            ) : (
              <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs font-bold bg-red-500/10 text-red-400 border border-red-500/20">
                <AlertTriangle size={12} /> Offline
              </span>
            )}
          </div>
        </div>

        {/* Inference Backend & Model probe */}
        <div className="p-4 bg-slate-950/60 rounded-xl border border-slate-800 flex flex-col justify-between">
          <div className="flex items-center justify-between mb-2">
            <div className="flex items-center gap-2 text-slate-300 font-bold text-sm">
              <Layers size={16} className="text-cyan-400" /> Loaded Model
            </div>
            <span className="text-[10px] font-mono text-slate-500">:8080</span>
          </div>
          <div className="mt-2">
            {inferenceBackendStatus === 'checking' ? (
              <span className="text-xs text-slate-400 font-mono">Checking...</span>
            ) : loadedModel ? (
              <div className="truncate font-mono text-xs text-cyan-300 font-bold" title={loadedModel}>
                {loadedModel}
              </div>
            ) : (
              <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs font-bold bg-slate-800 text-slate-400">
                No model loaded
              </span>
            )}
          </div>
        </div>
      </div>

      {/* Actionable Recovery Options */}
      {(controlPlaneStatus === 'unauthorized' || internalApiStatus === 'unauthorized') && (
        <form onSubmit={handleApplyApiKey} className="p-4 bg-amber-500/5 border border-amber-500/20 rounded-xl space-y-3">
          <div className="flex items-center gap-2 text-amber-400 text-sm font-bold">
            <Key size={16} /> Authentication Recovery (HTTP 401)
          </div>
          <p className="text-xs text-slate-400">
            The backend rejected requests with 401 Unauthorized. Provide a valid Admin or Operator API key:
          </p>
          <div className="flex gap-2">
            <input
              type="password"
              placeholder="Paste Bearer API key..."
              value={inputApiKey}
              onChange={(e) => setInputApiKey(e.target.value)}
              className="flex-1 px-3 py-2 bg-slate-950 border border-slate-800 rounded-lg text-xs text-slate-200 font-mono focus:outline-none focus:border-amber-500"
            />
            <button
              type="submit"
              className="px-4 py-2 bg-amber-600 hover:bg-amber-500 text-white rounded-lg text-xs font-bold transition focus-visible:ring-2 focus-visible:ring-amber-500"
            >
              Apply Key
            </button>
          </div>
        </form>
      )}

      {/* Navigation & Base URL CTAs */}
      <div className="flex flex-wrap items-center justify-between gap-4 pt-2 border-t border-slate-800/80">
        <div className="flex items-center gap-2">
          {onNavigateToTab && (
            <button
              onClick={() => onNavigateToTab('models')}
              className="px-3 py-1.5 bg-blue-600 hover:bg-blue-500 text-white rounded-lg text-xs font-bold transition focus-visible:ring-2 focus-visible:ring-blue-500"
            >
              Go to Models Tab
            </button>
          )}
          {onNavigateToTab && (
            <button
              onClick={() => onNavigateToTab('settings')}
              className="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-lg text-xs font-bold transition focus-visible:ring-2 focus-visible:ring-blue-500"
            >
              System Settings
            </button>
          )}
        </div>
        <div className="text-xs text-slate-500 font-mono">
          API Base: <span className="text-slate-400">{currentApiBase}</span> (Control-plane Gateway)
        </div>
      </div>
    </div>
  );
};
