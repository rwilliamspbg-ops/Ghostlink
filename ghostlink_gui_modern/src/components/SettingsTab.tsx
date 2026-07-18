import React, { useState, useEffect, useCallback } from 'react';
import {
  Settings,
  Cpu,
  Sliders,
  Network,
  Server,
  Zap,
  Save,
  RotateCcw,
  CheckCircle2,
  AlertTriangle,
  Loader,
  BarChart3,
} from 'lucide-react';
import { Settings as SettingsType } from '../store';

export const SettingsTab: React.FC<{ api: any }> = ({ api }) => {
  const [settings, setSettings] = useState<SettingsType | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState('');
  const [validationErrors, setValidationErrors] = useState<Record<string, string>>({});
  
  // Phase 5: Backend selector state
  const [backends, setBackends] = useState<any[]>([]);
  const [currentBackend, setCurrentBackend] = useState('cpu');
  const [backendLoading, setBackendLoading] = useState(false);
  const [backendSwitching, setBackendSwitching] = useState(false);
  const [backendError, setBackendError] = useState('');
  const [backendSuccess, setBackendSuccess] = useState('');

  const validate = () => {
    if (!settings) return true;
    const errs: Record<string, string> = {};
    if (settings.api_port && (settings.api_port < 1024 || settings.api_port > 65535)) errs.api_port = 'Port must be 1024-65535';
    if (settings.gui_port && (settings.gui_port < 1024 || settings.gui_port > 65535)) errs.gui_port = 'Port must be 1024-65535';
    if (settings.temperature !== undefined && (settings.temperature < 0 || settings.temperature > 2)) errs.temperature = 'Temperature must be 0-2';
    if (settings.top_p !== undefined && (settings.top_p < 0 || settings.top_p > 1)) errs.top_p = 'Top P must be 0-1';
    if (settings.api_host && !/^[\w.*:-]+$/.test(settings.api_host)) errs.api_host = 'Invalid host format';
    setValidationErrors(errs);
    return Object.keys(errs).length === 0;
  };

  const handleSave = async () => {
    if (!settings || !validate()) return;
    setSaving(true);
    setSaved(false);
    const result = await api.updateSettings(settings);
    if (result.success) {
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } else {
      setError(result.error || 'Save failed');
    }
    setSaving(false);
  };

  const loadSettings = useCallback(async () => {
    setLoading(true);
    const result = await api.getSettings();
    if (result.settings && !result.error) {
      setSettings(result.settings);
    } else {
      setError(result.error || 'Failed to load settings');
    }
    setLoading(false);
  }, [api]);

  // Phase 5: Load backends on mount
  const loadBackends = useCallback(async () => {
    setBackendLoading(true);
    setBackendError('');
    const result = await api.getBackends();
    if (!result.error) {
      setBackends(result.backends);
      setCurrentBackend(result.current);
    } else {
      setBackendError(result.error);
    }
    setBackendLoading(false);
  }, [api]);

  // Phase 5: Switch backend
  const handleBackendSwitch = async (newBackend: string) => {
    setBackendSwitching(true);
    setBackendError('');
    setBackendSuccess('');

    const result = await api.switchBackend(newBackend);
    if (result.success) {
      setCurrentBackend(newBackend);
      setBackendSuccess(`Switched to ${newBackend} backend`);
      setTimeout(() => setBackendSuccess(''), 3000);
    } else {
      setBackendError(result.error || 'Failed to switch backend');
    }
    setBackendSwitching(false);
  };

  useEffect(() => {
    loadSettings();
    loadBackends();
  }, [loadSettings, loadBackends]);

  const update = (key: string, value: any) => {
    if (!settings) return;
    setSettings({ ...settings, [key]: value });
  };

  const handleReset = async () => {
    setSaving(true);
    const result = await api.resetSettings();
    if (result.success && result.settings) {
      setSettings(result.settings);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    }
    setSaving(false);
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full text-slate-400">
        <div className="text-center">
          <div className="mb-4 text-sm">Loading settings...</div>
          <div className="w-8 h-8 border-2 border-slate-700 border-t-blue-600 rounded-full animate-spin mx-auto"></div>
        </div>
      </div>
    );
  }

  if (!settings) {
    return (
      <div className="flex items-center justify-center h-full text-slate-400">
        <div className="text-center">
          <AlertTriangle className="mx-auto mb-2 text-orange-400" size={24} />
          <div className="text-sm text-red-400">{error || 'Failed to load settings'}</div>
        </div>
      </div>
    );
  }

  const Section = ({ title, icon: Icon, children }: { title: string; icon: any; children: React.ReactNode }) => (
    <div className="bg-slate-900/50 border border-slate-800 rounded-3xl p-6 space-y-6">
      <div className="flex items-center gap-3">
        <div className="p-3 bg-blue-500/10 rounded-2xl text-blue-400">
          <Icon size={24} />
        </div>
        <h3 className="font-bold text-slate-100">{title}</h3>
      </div>
      {children}
    </div>
  );

  const SliderField = ({ label, desc, value, min, max, step, onChange, unit }: {
    label: string; desc?: string; value: number; min: number; max: number; step?: number; onChange: (v: number) => void; unit?: string;
  }) => {
    const fieldId = label.toLowerCase().replace(/\s+/g, '-');
    return (
      <div className="space-y-1.5">
        <div className="flex items-center justify-between">
          <label htmlFor={fieldId} className="text-sm font-medium text-slate-300">{label}</label>
          <span className="text-xs font-mono text-blue-400 bg-blue-500/10 px-2 py-0.5 rounded-md">{value}{unit}</span>
        </div>
        {desc && <p className="text-[10px] text-slate-500">{desc}</p>}
        <input
          id={fieldId}
          name={fieldId}
          type="range"
          min={min}
          max={max}
          step={step || 1}
          value={value}
          onChange={(e) => onChange(parseFloat(e.target.value))}
          className="w-full h-1.5 bg-slate-700 rounded-lg appearance-none cursor-pointer accent-blue-500"
        />
        <div className="flex justify-between text-[10px] text-slate-600">
          <span>{min}</span>
          <span>{max}</span>
        </div>
      </div>
    );
  };

  const SelectField = ({ label, desc, value, options, onChange }: {
    label: string; desc?: string; value: string; options: { value: string; label: string }[]; onChange: (v: string) => void;
  }) => {
    const fieldId = label.toLowerCase().replace(/\s+/g, '-');
    return (
      <div className="space-y-1.5">
        <label htmlFor={fieldId} className="text-sm font-medium text-slate-300">{label}</label>
        {desc && <p className="text-[10px] text-slate-500">{desc}</p>}
        <select
          id={fieldId}
          name={fieldId}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className="w-full bg-slate-800 border border-slate-700 rounded-xl px-3 py-2.5 text-sm text-slate-200 focus:outline-none focus:ring-2 focus:ring-blue-500/50 focus:border-blue-500/50"
        >
          {options.map((o) => (
            <option key={o.value} value={o.value}>{o.label}</option>
          ))}
        </select>
      </div>
    );
  };

  const InputField = ({ label, desc, value, onChange, type = 'text', placeholder }: {
    label: string; desc?: string; value: string; onChange: (v: string) => void; type?: string; placeholder?: string;
  }) => {
    const fieldId = label.toLowerCase().replace(/\s+/g, '-');
    return (
      <div className="space-y-1.5">
        <label htmlFor={fieldId} className="text-sm font-medium text-slate-300">{label}</label>
        {desc && <p className="text-[10px] text-slate-500">{desc}</p>}
        <input
          id={fieldId}
          name={fieldId}
          type={type}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={placeholder}
          className="w-full bg-slate-800 border border-slate-700 rounded-xl px-3 py-2.5 text-sm text-slate-200 focus:outline-none focus:ring-2 focus:ring-blue-500/50 focus:border-blue-500/50 font-mono"
        />
      </div>
    );
  };

  return (
    <div className="flex flex-col h-full bg-slate-950">
      <div className="flex items-center justify-between px-6 py-4 border-b border-slate-900 sticky top-0 bg-slate-950/50 backdrop-blur-md z-10">
        <div className="flex items-center gap-3">
          <Settings size={20} className="text-blue-400" />
          <h2 className="text-xl font-bold text-white">Runtime Settings</h2>
        </div>
        <div className="flex items-center gap-2">
          {Object.keys(validationErrors).length > 0 && (
            <span className="flex items-center gap-1 text-xs text-orange-400 bg-orange-500/10 px-2 py-1 rounded-lg">
              <AlertTriangle size={14} /> Fix validation errors
            </span>
          )}
          {saved && (
            <span className="flex items-center gap-1 text-xs text-green-400 bg-green-500/10 px-2 py-1 rounded-lg">
              <CheckCircle2 size={14} /> Saved
            </span>
          )}
          <button
            onClick={handleReset}
            disabled={saving}
            className="flex items-center gap-2 px-3 py-2 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-xl text-sm transition disabled:opacity-50"
          >
            <RotateCcw size={14} /> Reset
          </button>
          <button
            onClick={handleSave}
            disabled={saving}
            className="flex items-center gap-2 px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-xl text-sm font-bold transition shadow-lg shadow-blue-500/20 disabled:opacity-50"
          >
            <Save size={14} /> {saving ? 'Saving...' : 'Save'}
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        <div className="max-w-5xl mx-auto space-y-6">
          {/* Phase 5: GPU/CPU Compute Backend Section */}
          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            <Section title="Compute Backend" icon={BarChart3}>
              {backendError && (
                <div className="flex items-center gap-2 text-xs text-red-400 bg-red-500/10 px-3 py-2 rounded-lg mb-3">
                  <AlertTriangle size={14} /> {backendError}
                </div>
              )}
              {backendSuccess && (
                <div className="flex items-center gap-2 text-xs text-green-400 bg-green-500/10 px-3 py-2 rounded-lg mb-3">
                  <CheckCircle2 size={14} /> {backendSuccess}
                </div>
              )}
              
              {backendLoading ? (
                <div className="flex items-center justify-center py-4">
                  <Loader size={16} className="animate-spin text-blue-400 mr-2" />
                  <span className="text-sm text-slate-400">Loading backends...</span>
                </div>
              ) : (
                <div className="space-y-3">
                  <div className="grid grid-cols-1 gap-2">
                    {backends.length > 0 ? (
                      backends.map((backend: any) => {
                        const displayVram = backend.vram_gb ? Number(backend.vram_gb).toFixed(1) : 'N/A';
                        const displayCapability = backend.compute_capability || 'N/A';
                        return (
                        <button
                          key={backend.name}
                          onClick={() => handleBackendSwitch(backend.name)}
                          disabled={backendSwitching || currentBackend === backend.name}
                          className={`p-3 rounded-xl text-left text-sm transition ${
                            currentBackend === backend.name
                              ? 'bg-blue-600 border border-blue-500 text-white'
                              : backendSwitching
                              ? 'bg-slate-800 border border-slate-700 text-slate-400 opacity-50 cursor-not-allowed'
                              : 'bg-slate-800 border border-slate-700 text-slate-300 hover:bg-slate-700 hover:border-slate-600'
                          }`}
                        >
                          <div className="flex items-center justify-between">
                            <div>
                              <div className="font-semibold capitalize">{backend.name}</div>
                              <div className={`text-[10px] ${currentBackend === backend.name ? 'text-blue-200' : 'text-slate-500'}`}>
                                {backend.device_name || 'Unknown'} • {displayVram}GB • {displayCapability}
                              </div>
                            </div>
                            {currentBackend === backend.name && (
                              <CheckCircle2 size={16} className="flex-shrink-0" />
                            )}
                            {backendSwitching && currentBackend !== backend.name && backend.name === 'cpu' && (
                              <Loader size={16} className="animate-spin flex-shrink-0" />
                            )}
                          </div>
                        </button>
                        );
                      })
                    ) : (
                      <div className="text-center py-4 text-slate-500 text-sm">
                        No backends available
                      </div>
                    )}
                  </div>
                  <p className="text-[10px] text-slate-500 bg-slate-800/50 px-3 py-2 rounded-lg">
                    Current: <span className="text-slate-300 font-semibold capitalize">{currentBackend}</span> backend
                  </p>
                </div>
              )}
            </Section>

            {/* Runtime Section */}
            <Section title="Inference Runtime" icon={Cpu}>
              <SelectField
                label="Backend"
                desc="Inference engine backend"
                value={settings.inference_backend}
                options={[
                  { value: 'ollama', label: 'Ollama' },
                  { value: 'native', label: 'Native (legacy)' },
                ]}
                onChange={(v) => update('inference_backend', v)}
              />
            </Section>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            {/* Sampling Section */}
            <Section title="Sampling Parameters" icon={Sliders}>
              <SliderField
                label="Temperature"
                desc="Randomness of output (0 = deterministic, 2 = very random)"
                value={settings.temperature}
                min={0} max={2} step={0.05}
                onChange={(v) => update('temperature', v)}
              />
              <SliderField
                label="Top P"
                desc="Nucleus sampling threshold"
                value={settings.top_p}
                min={0} max={1} step={0.05}
                onChange={(v) => update('top_p', v)}
              />
              <SliderField
                label="Top K"
                desc="Top-K sampling (number of tokens to consider)"
                value={settings.top_k}
                min={1} max={200} step={1}
                onChange={(v) => update('top_k', v)}
              />
              <SliderField
                label="Repeat Penalty"
                desc="Penalize repeated tokens"
                value={settings.repeat_penalty}
                min={0} max={2} step={0.05}
                onChange={(v) => update('repeat_penalty', v)}
              />
              <SliderField
                label="Max Tokens"
                desc="Maximum tokens per response"
                value={settings.max_tokens}
                min={16} max={8192} step={16}
                onChange={(v) => update('max_tokens', v)}
              />
            </Section>

            {/* Model Section */}
            <Section title="Inference Parameters" icon={Server}>
              <SliderField
                label="Chat Exec Tokens"
                desc="Execution token budget per chat request"
                value={settings.chat_exec_tokens}
                min={64} max={8192} step={64}
                onChange={(v) => update('chat_exec_tokens', v)}
              />
              <SliderField
                label="Micro Batch"
                desc="Chat micro batch size"
                value={settings.chat_micro_batch}
                min={1} max={32} step={1}
                onChange={(v) => update('chat_micro_batch', v)}
              />
            </Section>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            {/* Network Section */}
            <Section title="Network" icon={Network}>
              <InputField
                label="API Host"
                desc="Backend API bind address"
                value={settings.api_host}
                onChange={(v) => update('api_host', v)}
              />
              <SliderField
                label="API Port"
                desc="Backend API server port"
                value={settings.api_port}
                min={1024} max={65535} step={1}
                onChange={(v) => update('api_port', v)}
              />
              <SliderField
                label="GUI Port"
                desc="React frontend dev server port"
                value={settings.gui_port}
                min={1024} max={65535} step={1}
                onChange={(v) => update('gui_port', v)}
              />
              <InputField
                label="Discovery Listen"
                desc="UDP discovery bind address"
                value={settings.discovery_listen}
                onChange={(v) => update('discovery_listen', v)}
              />
              <InputField
                label="Discovery Broadcast"
                desc="UDP discovery broadcast address"
                value={settings.discovery_broadcast}
                onChange={(v) => update('discovery_broadcast', v)}
              />
              <InputField
                label="Discovery Auth Token"
                desc="Authentication token for peer discovery"
                value={settings.discovery_auth_token}
                onChange={(v) => update('discovery_auth_token', v)}
                type="password"
              />
              <InputField
                label="TCP Auth Token"
                desc="Authentication token for TCP transport"
                value={settings.tcp_auth_token}
                onChange={(v) => update('tcp_auth_token', v)}
                type="password"
              />
            </Section>

            {/* Performance Section */}
            <Section title="Performance" icon={Zap}>
              <SliderField
                label="TCP Max Inflight"
                desc="Maximum in-flight TCP transport requests"
                value={settings.tcp_max_inflight}
                min={16} max={4096} step={16}
                onChange={(v) => update('tcp_max_inflight', v)}
              />
              <InputField
                label="XDP Interface"
                desc="Network interface for XDP kernel bypass"
                value={settings.xdp_interface}
                onChange={(v) => update('xdp_interface', v)}
                placeholder="eth0"
              />
            </Section>
          </div>

          <div className="bg-slate-900/50 border border-slate-800/50 rounded-3xl p-4">
            <p className="text-[10px] text-slate-500 text-center">
              Settings are persisted to <code className="text-blue-400 bg-slate-800 px-1.5 py-0.5 rounded">settings.json</code>.
              Some settings require a service restart to take effect.
            </p>
          </div>
        </div>
      </div>
    </div>
  );
};
