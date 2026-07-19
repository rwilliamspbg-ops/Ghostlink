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
  RefreshCw,
  Gauge,
  Info,
  Cpu as CpuIcon,
  Loader2,
} from 'lucide-react';
import { Settings as SettingsType } from '../store';
import { GhostlinkAPI } from '../api';

type BackendInfo = {
  name: string;
  device_name: string;
  vram_gb: number | null;
  compute_capability: string;
  driver_version: string;
  status: string;
};

export const SettingsTab: React.FC<{ api: GhostlinkAPI }> = ({ api }) => {
  const [settings, setSettings] = useState<SettingsType | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState('');
  const [validationErrors, setValidationErrors] = useState<Record<string, string>>({});
  const [backends, setBackends] = useState<BackendInfo[]>([]);
  const [currentBackend, setCurrentBackend] = useState<string>('cpu');
  const [backendLoading, setBackendLoading] = useState(false);
  const [switchingBackend, setSwitchingBackend] = useState<string | null>(null);
  const [restartRequired, setRestartRequired] = useState(false);

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

  const loadBackends = useCallback(async () => {
    setBackendLoading(true);
    try {
      const result = await api.getBackends();
      if (!result.error && result.available) {
        setBackends(result.available);
        setCurrentBackend(result.current);
      }
    } catch (e: any) {
      console.error('Failed to load backends:', e);
    }
    setBackendLoading(false);
  }, [api]);

  const handleSwitchBackend = async (backendName: string) => {
    setSwitchingBackend(backendName);
    try {
      const result = await api.switchBackend(backendName);
      if (result.success) {
        setCurrentBackend(backendName);
        setBackends(prev => prev.map(b => ({
          ...b,
          status: b.name === backendName ? 'active' : 'ready'
        })));
        if (result.restart_required) {
          setRestartRequired(true);
        }
      } else {
        setError(result.error || 'Failed to switch backend');
      }
    } catch (e: any) {
      setError(e.message);
    }
    setSwitchingBackend(null);
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

  useEffect(() => {
    loadSettings();
    loadBackends();
  }, [loadSettings, loadBackends]);

  const update = (key: string, value: any) => {
    if (!settings) return;
    setSettings({ ...settings, [key]: value });
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
          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            <Section title="Compute Backend" icon={Gauge}>
              <div className="space-y-4">
                {backendLoading ? (
                  <div className="flex items-center justify-center py-8">
                    <Loader2 size={24} className="text-blue-400 animate-spin" />
                    <span className="ml-3 text-slate-400">Detecting backends...</span>
                  </div>
                ) : (
                  <>
                    <div className="rounded-2xl border border-slate-800 bg-slate-950/60 p-4 space-y-3">
                      <div className="flex items-center justify-between gap-3">
                        <div className="flex items-center gap-3">
                          <div className="p-2 bg-blue-500/10 rounded-xl text-blue-400">
                            <CpuIcon size={20} />
                          </div>
                          <div>
                            <p className="text-xs uppercase tracking-wider text-slate-500">Current Backend</p>
                            <p className="text-lg font-bold text-white capitalize">{currentBackend}</p>
                          </div>
                        </div>
                        <button
                          onClick={loadBackends}
                          disabled={backendLoading}
                          className="flex items-center gap-2 px-3 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 text-sm transition disabled:opacity-50"
                        >
                          <RefreshCw size={14} className={backendLoading ? 'animate-spin' : ''} />
                          Re-detect
                        </button>
                      </div>

                      <div className="space-y-2">
                        {backends.map((backend) => (
                          <button
                            key={backend.name}
                            onClick={() => handleSwitchBackend(backend.name)}
                            disabled={switchingBackend !== null || backend.name === currentBackend}
                            className={`flex items-center justify-between w-full px-3 py-2.5 rounded-xl text-left transition text-sm ${
                              backend.name === currentBackend
                                ? 'bg-blue-600/10 text-blue-400 border border-blue-500/30'
                                : 'text-slate-300 hover:bg-slate-800 border border-slate-800'
                            } ${switchingBackend === backend.name ? 'opacity-50' : ''}`}
                          >
                            <div className="flex items-center gap-3 min-w-0">
                              <div className={`flex items-center justify-center h-8 w-8 rounded-lg ${
                                backend.name === 'cuda' ? 'bg-green-500/20 text-green-400' :
                                backend.name === 'rocm' ? 'bg-red-500/20 text-red-400' :
                                backend.name === 'metal' ? 'bg-orange-500/20 text-orange-400' :
                                backend.name === 'directml' ? 'bg-purple-500/20 text-purple-400' :
                                backend.name === 'npu' ? 'bg-yellow-500/20 text-yellow-400' :
                                'bg-slate-500/20 text-slate-400'
                              }`}>
                                <CpuIcon size={16} />
                              </div>
                              <div className="min-w-0">
                                <div className="font-bold truncate capitalize">{backend.name}</div>
                                <div className="text-[10px] text-slate-500 truncate">{backend.device_name}</div>
                              </div>
                            </div>
                            <div className="flex items-center gap-2">
                              {backend.vram_gb != null && (
                                <span className="text-xs text-slate-400 px-2 py-0.5 bg-slate-800 rounded">
                                  {backend.vram_gb.toFixed(1)} GB VRAM
                                </span>
                              )}
                              <span className={`px-2 py-0.5 rounded text-xs font-medium ${
                                backend.name === currentBackend
                                  ? 'bg-blue-500/20 text-blue-400'
                                  : backend.status === 'ready'
                                  ? 'bg-green-500/20 text-green-400'
                                  : 'bg-slate-700 text-slate-500'
                              }`}>
                                {backend.name === currentBackend ? 'Active' : backend.status}
                              </span>
                              {switchingBackend === backend.name && <Loader2 size={14} className="animate-spin" />}
                            </div>
                          </button>
                        ))}
                      </div>
                    </div>

                    {restartRequired && (
                      <div className="flex items-center gap-2 px-4 py-3 bg-orange-500/10 border border-orange-500/30 rounded-xl text-orange-400">
                        <AlertTriangle size={20} />
                        <div>
                          <p className="font-medium">Restart Required</p>
                          <p className="text-sm text-slate-400">The backend has been switched but requires a server restart to take full effect.</p>
                        </div>
                      </div>
                    )}

                    <div className="flex items-center gap-2 text-xs text-slate-500 bg-slate-900/50 rounded-xl p-3">
                      <Info size={14} />
                      <span>
                        Backend is auto-detected at startup. Override with <code className="text-blue-400 bg-slate-800 px-1.5 py-0.5 rounded font-mono">GHOSTLINK_INFERENCE_BACKEND=native|ollama</code> env var.
                        {' '}
                        <a href="https://github.com/ghostlink/ghostlink/blob/main/docs/RUNTIMES.md" target="_blank" rel="noopener noreferrer" className="text-blue-400 hover:underline ml-1">
                          Docs
                        </a>
                      </span>
                    </div>
                  </>
                )}
              </div>
            </Section>

            {/* Inference Runtime Section */}
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