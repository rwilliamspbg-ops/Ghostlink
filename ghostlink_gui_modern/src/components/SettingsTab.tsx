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
  Layers,
} from 'lucide-react';
import { Settings as SettingsType } from '../store';
import { GhostlinkAPI } from '../api';
import { useInferenceEngines } from '../hooks/useInferenceEngines';

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
  const { engines: inferenceEngines, selectedEngine, engineHealth } = useInferenceEngines(
    api,
    settings?.inference_backend || null
  );
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
    if (settings.inference_backend === 'vllm' && settings.vllm_base_url && !/^https?:\/\//.test(settings.vllm_base_url)) errs.vllm_base_url = 'vLLM URL must start with http:// or https://';
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
    if (!window.confirm('Are you sure you want to reset all settings to defaults? This will overwrite your current configuration.')) {
      return;
    }
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

  // For the Auto/Manual tuning controls below: flipping "Auto" off needs to
  // seed a sane starting value in the same update, not just flip the flag —
  // otherwise unchecking Auto would save `ngl: 0`/`ctx_size: 0` (whatever
  // the field happened to hold, likely never edited) as the new explicit
  // override the moment the user saves.
  const updateFields = (patch: Partial<SettingsType>) => {
    if (!settings) return;
    setSettings({ ...settings, ...patch });
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

  const engineOptions = inferenceEngines.map((engine) => ({ value: engine.name, label: engine.label }));
  const capabilityRows = selectedEngine
    ? [
        { label: 'Streaming', enabled: selectedEngine.capabilities.streaming },
        { label: 'Model listing', enabled: selectedEngine.capabilities.model_listing },
        { label: 'Model load', enabled: selectedEngine.capabilities.model_load },
        { label: 'Model unload', enabled: selectedEngine.capabilities.model_unload },
        { label: 'Structured outputs', enabled: selectedEngine.capabilities.structured_outputs },
        { label: 'Tool calls', enabled: selectedEngine.capabilities.tool_calls },
      ]
    : [];

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

  const SliderField = ({ label, desc, value, min, max, step, onChange, unit, auto, onAutoChange, autoNote }: {
    label: string; desc?: string; value: number; min: number; max: number; step?: number; onChange: (v: number) => void; unit?: string;
    /** Optional "Auto" checkbox in the header row. When `auto` is true, the
     * slider itself is replaced by an explanatory note instead of being
     * disabled-but-visible, so it's unambiguous (including to a screen
     * reader) that this control isn't the thing currently in effect. */
    auto?: boolean; onAutoChange?: (v: boolean) => void; autoNote?: string;
  }) => {
    const fieldId = label.toLowerCase().replace(/\s+/g, '-');
    const hasAuto = onAutoChange !== undefined;
    // Settings round-trip through an f32 backend, so fractional values arrive
    // as things like 0.8999999761581421 — round for display to the precision
    // `step` actually offers instead of showing the float32 artifact.
    const displayValue = value === undefined || value === null
      ? '0'
      : (!step || step >= 1
        ? Math.round(value).toString()
        : value.toFixed(Math.max(0, -Math.floor(Math.log10(step)))));
    return (
      <div className="space-y-1.5">
        <div className="flex items-center justify-between gap-2">
          <label htmlFor={fieldId} className="text-sm font-medium text-slate-300">{label}</label>
          <div className="flex items-center gap-2">
            {hasAuto && (
              <label htmlFor={`${fieldId}-auto`} className="flex items-center gap-1.5 text-xs text-slate-400 cursor-pointer select-none">
                <input
                  id={`${fieldId}-auto`}
                  name={`${fieldId}-auto`}
                  type="checkbox"
                  checked={!!auto}
                  onChange={(e) => onAutoChange!(e.target.checked)}
                  title={`Auto: let Ghostlink choose ${label.toLowerCase()} automatically from detected hardware and model size, instead of the fixed value below`}
                  className="rounded border-slate-700 bg-slate-800 text-blue-500 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                />
                Auto
              </label>
            )}
            {!(hasAuto && auto) && (
              <span className="text-xs font-mono text-blue-400 bg-blue-500/10 px-2 py-0.5 rounded-md">{displayValue}{unit}</span>
            )}
          </div>
        </div>
        {desc && <p className="text-[10px] text-slate-500">{desc}</p>}
        {hasAuto && auto ? (
          <p className="text-xs text-slate-500 italic bg-slate-900/50 rounded-xl px-3 py-2">
            {autoNote || 'Decided automatically at model-load time from detected VRAM and model size.'}
          </p>
        ) : (
          <>
            <input
              id={fieldId}
              name={fieldId}
              type="range"
              min={min}
              max={max}
              step={step || 1}
              value={value ?? 0}
              onChange={(e) => onChange(parseFloat(e.target.value))}
              className="w-full h-1.5 bg-slate-700 rounded-lg appearance-none cursor-pointer accent-blue-500 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
            />
            <div className="flex justify-between text-[10px] text-slate-600">
              <span>{min}</span>
              <span>{max}</span>
            </div>
          </>
        )}
      </div>
    );
  };

  const ToggleField = ({ label, desc, checked, onChange, warning, disabled }: {
    label: string; desc?: string; checked: boolean; onChange: (v: boolean) => void; warning?: string; disabled?: boolean;
  }) => {
    const fieldId = label.toLowerCase().replace(/\s+/g, '-');
    return (
      <div className="space-y-1.5">
        <label htmlFor={fieldId} className={`flex items-center justify-between gap-3 ${disabled ? 'opacity-50' : 'cursor-pointer select-none'}`}>
          <span className="text-sm font-medium text-slate-300">{label}</span>
          <input
            id={fieldId}
            name={fieldId}
            type="checkbox"
            checked={checked}
            disabled={disabled}
            onChange={(e) => onChange(e.target.checked)}
            title={label}
            className="h-4 w-4 rounded border-slate-700 bg-slate-800 text-blue-500 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
          />
        </label>
        {desc && <p className="text-[10px] text-slate-500">{desc}</p>}
        {warning && (
          <div role="status" className="flex items-start gap-2 text-xs text-orange-400 bg-orange-500/10 px-3 py-2 rounded-xl">
            <AlertTriangle size={14} className="mt-0.5 shrink-0" aria-hidden="true" />
            <span>{warning}</span>
          </div>
        )}
      </div>
    );
  };

  const TriStateField = ({ label, desc, value, onChange }: {
    label: string; desc?: string; value: boolean | null; onChange: (v: boolean | null) => void;
  }) => {
    const options: { key: string; optValue: boolean | null; text: string }[] = [
      { key: 'auto', optValue: null, text: 'Auto' },
      { key: 'on', optValue: true, text: 'On' },
      { key: 'off', optValue: false, text: 'Off' },
    ];
    return (
      <div className="space-y-1.5">
        <span className="text-sm font-medium text-slate-300">{label}</span>
        {desc && <p className="text-[10px] text-slate-500">{desc}</p>}
        <div className="flex gap-2" role="radiogroup" aria-label={label}>
          {options.map((opt) => (
            <button
              key={opt.key}
              type="button"
              role="radio"
              aria-checked={value === opt.optValue}
              onClick={() => onChange(opt.optValue)}
              title={`${label}: ${opt.text}`}
              className={`flex-1 px-3 py-2 rounded-xl text-xs font-medium transition border focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none ${
                value === opt.optValue
                  ? 'bg-blue-600/10 text-blue-400 border-blue-500/30'
                  : 'text-slate-300 hover:bg-slate-800 border-slate-800'
              }`}
            >
              {opt.text}
            </button>
          ))}
        </div>
      </div>
    );
  };

  const SelectField = ({ label, desc, value, options, onChange, disabled }: {
    label: string; desc?: string; value: string; options: { value: string; label: string }[]; onChange: (v: string) => void; disabled?: boolean;
  }) => {
    const fieldId = label.toLowerCase().replace(/\s+/g, '-');
    return (
      <div className="space-y-1.5">
        <label htmlFor={fieldId} className={`text-sm font-medium text-slate-300 ${disabled ? 'opacity-50' : ''}`}>{label}</label>
        {desc && <p className="text-[10px] text-slate-500">{desc}</p>}
        <select
          id={fieldId}
          name={fieldId}
          value={value ?? ''}
          onChange={(e) => onChange(e.target.value)}
          disabled={disabled}
          title={disabled ? `${label} (disabled — see description above)` : label}
          className="w-full bg-slate-800 border border-slate-700 rounded-xl px-3 py-2.5 text-sm text-slate-200 focus:outline-none focus:ring-2 focus:ring-blue-500/50 focus:border-blue-500/50 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none disabled:opacity-50 disabled:cursor-not-allowed"
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
          value={value ?? ''}
          onChange={(e) => onChange(e.target.value)}
          placeholder={placeholder}
          className="w-full bg-slate-800 border border-slate-700 rounded-xl px-3 py-2.5 text-sm text-slate-200 focus:outline-none focus:ring-2 focus:ring-blue-500/50 focus:border-blue-500/50 font-mono focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
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
            <span role="alert" className="flex items-center gap-1 text-xs text-orange-400 bg-orange-500/10 px-2 py-1 rounded-lg">
              <AlertTriangle size={14} aria-hidden="true" /> Fix validation errors
            </span>
          )}
          {saved && (
            <span role="status" className="flex items-center gap-1 text-xs text-green-400 bg-green-500/10 px-2 py-1 rounded-lg">
              <CheckCircle2 size={14} aria-hidden="true" /> Saved
            </span>
          )}
          <button
            onClick={handleReset}
            disabled={saving}
            title="Reset all settings to defaults"
            className="flex items-center gap-2 px-3 py-2 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-xl text-sm transition disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
          >
            <RotateCcw size={14} aria-hidden="true" /> Reset
          </button>
          <button
            onClick={handleSave}
            disabled={saving}
            title="Save changes to settings.json"
            className="flex items-center gap-2 px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-xl text-sm font-bold transition shadow-lg shadow-blue-500/20 disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
          >
            <Save size={14} aria-hidden="true" /> {saving ? 'Saving...' : 'Save'}
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-6" tabIndex={0} role="region" aria-label="Settings">
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
                          title="Re-detect compute backends"
                          className="flex items-center gap-2 px-3 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 text-sm transition disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                        >
                          <RefreshCw size={14} className={backendLoading ? 'animate-spin' : ''} />
                          Re-detect
                        </button>
                      </div>

                      <div className="space-y-2" role="radiogroup" aria-label="Compute backend">
                        {backends.map((backend) => (
                          <button
                            key={backend.name}
                            role="radio"
                            aria-checked={backend.name === currentBackend}
                            onClick={() => handleSwitchBackend(backend.name)}
                            disabled={switchingBackend !== null || backend.name === currentBackend}
                            title={`Switch compute backend to ${backend.name}`}
                            className={`flex items-center justify-between w-full px-3 py-2.5 rounded-xl text-left transition text-sm ${
                              backend.name === currentBackend
                                ? 'bg-blue-600/10 text-blue-400 border border-blue-500/30'
                                : 'text-slate-300 hover:bg-slate-800 border border-slate-800'
                            } ${switchingBackend === backend.name ? 'opacity-50' : ''} focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none`}
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
                      <div role="alert" className="flex items-center gap-2 px-4 py-3 bg-orange-500/10 border border-orange-500/30 rounded-xl text-orange-400">
                        <AlertTriangle size={20} aria-hidden="true" />
                        <div>
                          <p className="font-medium">Restart Required</p>
                          <p className="text-sm text-slate-400">The backend has been switched but requires a server restart to take full effect.</p>
                        </div>
                      </div>
                    )}

                    <div className="flex items-center gap-2 text-xs text-slate-500 bg-slate-900/50 rounded-xl p-3">
                      <Info size={14} />
                      <span>
                        Backend is auto-detected at startup. Override with <code className="text-blue-400 bg-slate-800 px-1.5 py-0.5 rounded font-mono">GHOSTLINK_INFERENCE_BACKEND=native|ollama|vllm</code> env var.
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
            <Section title="Inference Engine" icon={Cpu}>
              <SelectField
                label="Backend"
                desc="Inference engine backend"
                value={settings.inference_backend}
                options={engineOptions}
                onChange={(v) => update('inference_backend', v)}
              />

              {selectedEngine && (
                <div className="rounded-2xl border border-slate-800 bg-slate-950/60 p-4 space-y-3">
                  <div className="flex items-center justify-between gap-3">
                    <div>
                      <p className="text-xs uppercase tracking-wider text-slate-500">Selected Engine</p>
                      <p className="text-sm font-bold text-slate-100">{selectedEngine.label}</p>
                    </div>
                    <span className="px-2 py-0.5 rounded text-xs font-medium bg-blue-500/10 text-blue-400">
                      {selectedEngine.status}
                    </span>
                  </div>

                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-2 text-xs">
                    {capabilityRows.map(({ label, enabled }) => (
                      <div key={label} className="flex items-center justify-between px-3 py-2 rounded-xl bg-slate-900/70 border border-slate-800">
                        <span className="text-slate-400">{label}</span>
                        <span className={enabled ? 'text-emerald-400 font-medium' : 'text-slate-500'}>
                          {enabled ? 'Supported' : 'Not supported'}
                        </span>
                      </div>
                    ))}
                  </div>

                  {selectedEngine.default_base_url && (
                    <p className="text-[10px] text-slate-500">
                      Default endpoint: <span className="font-mono text-slate-300">{selectedEngine.default_base_url}</span>
                    </p>
                  )}

                  {(selectedEngine.name === 'ollama' || selectedEngine.name === 'vllm') && (
                    <div className="flex items-center justify-between px-3 py-2 rounded-xl bg-slate-900/70 border border-slate-800 text-xs">
                      <span className="text-slate-400">Connectivity probe</span>
                      <span className={engineHealth?.reachable ? 'text-emerald-400 font-medium' : 'text-orange-400 font-medium'}>
                        {engineHealth?.reachable ? `Reachable · ${engineHealth.model_count ?? 0} models` : 'Unavailable'}
                      </span>
                    </div>
                  )}
                </div>
              )}

              {settings.inference_backend === 'vllm' && (
                <InputField
                  label="vLLM Base URL"
                  desc="Base URL for the vLLM OpenAI-compatible server"
                  value={settings.vllm_base_url || ''}
                  onChange={(v) => update('vllm_base_url', v)}
                  placeholder="http://127.0.0.1:8000"
                />
              )}

              {validationErrors.vllm_base_url && (
                <p className="text-xs text-red-400">{validationErrors.vllm_base_url}</p>
              )}

              {settings.inference_backend === 'vllm' && (
                <InputField
                  label="vLLM API Key"
                  desc="Optional bearer token for protected vLLM deployments"
                  value={settings.vllm_api_key || ''}
                  onChange={(v) => update('vllm_api_key', v)}
                  type="password"
                />
              )}
            </Section>

            {/* Runtime Section */}
            <Section title="Engine Connection" icon={Cpu}>
              {settings.inference_backend === 'native' ? (
                <InputField
                  label="Llama Server URL"
                  desc="Native engine endpoint used by the lightweight local path"
                  value={settings.llama_server_url || ''}
                  onChange={(v) => update('llama_server_url', v)}
                  placeholder="http://127.0.0.1:8080/completion"
                />
              ) : settings.inference_backend === 'vllm' ? (
                <div className="rounded-2xl border border-slate-800 bg-slate-950/60 p-4 text-sm text-slate-400">
                  vLLM uses the configured OpenAI-compatible endpoint and inherits model availability from the remote server.
                </div>
              ) : (
                <div className="rounded-2xl border border-slate-800 bg-slate-950/60 p-4 text-sm text-slate-400">
                  Ollama uses the local daemon connection configured by the launcher or <span className="font-mono text-slate-300">OLLAMA_BASE_URL</span>.
                </div>
              )}
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
              <SliderField
                label="Conversation Token Limit"
                desc="How much chat history the model remembers — separate from Max Tokens, which only caps the reply. Oldest turns are dropped first once a conversation outgrows this."
                value={settings.conversation_token_limit}
                min={256} max={16384} step={128}
                onChange={(v) => update('conversation_token_limit', v)}
              />
              {settings.conversation_token_limit + settings.max_tokens > 4096 && (
                <div role="alert" className="flex items-start gap-2 text-xs text-orange-400 bg-orange-500/10 px-3 py-2 rounded-xl">
                  <AlertTriangle size={14} className="mt-0.5 shrink-0" aria-hidden="true" />
                  <span>
                    History ({settings.conversation_token_limit}) + Max Tokens ({settings.max_tokens}) exceeds the
                    default 4096-token context window. If your model runs with a larger <code className="text-orange-300 bg-slate-800 px-1 rounded font-mono">GHOSTLINK_CTX_SIZE</code>, this is fine — otherwise the server-side truncation still protects you.
                  </span>
                </div>
              )}
            </Section>

            {/* Model Section */}
            <Section title="Inference Parameters" icon={Server}>
              <SliderField
                label="Parallel Slots"
                desc="Concurrent llama-server inference slots. 1 processes one generation at a time (today's default); raising it lets the server handle multiple requests concurrently instead of queueing them, at the cost of splitting VRAM/context across slots. Takes effect on the next model load."
                value={settings.parallel_slots}
                min={1} max={16} step={1}
                onChange={(v) => update('parallel_slots', v)}
              />
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

            {/* Model Performance Section */}
            <Section title="Model Performance" icon={Layers}>
              {settings.inference_backend !== 'native' ? (
                <div className="rounded-2xl border border-slate-800 bg-slate-950/60 p-4 text-sm text-slate-400">
                  These controls configure the native llama-server engine (`-ngl`, `-c`, `-b`/`-ub`, KV cache
                  quantization, mlock/mmap) and have no effect on the {settings.inference_backend === 'vllm' ? 'vLLM' : 'Ollama'} backend
                  currently selected above — that runtime manages its own model loading. Switch{' '}
                  <strong className="text-slate-300">Inference Engine → Backend</strong> to Native to use them.
                </div>
              ) : (
                <>
                  <p className="text-[10px] text-slate-500 -mt-2">
                    Every control here defaults to <strong className="text-slate-400">Auto</strong> — Ghostlink already
                    picks these from detected VRAM and the loaded model's size, including a safety cap that keeps large
                    models from exhausting memory. Only switch one to a fixed value if you've measured it helps on your
                    hardware. Takes effect on the <em>next</em> model load, not the current one.
                  </p>
                  <SliderField
                label="GPU Offload"
                desc="Number of model layers to run on the GPU (-ngl). Auto offloads based on detected VRAM, capped toward CPU-only for large models on shared-memory (iGPU) hardware to avoid exhausting system RAM."
                value={settings.ngl < 0 ? 0 : settings.ngl}
                min={0} max={100} step={1}
                onChange={(v) => update('ngl', v)}
                auto={settings.ngl_auto}
                onAutoChange={(a) => updateFields({ ngl_auto: a, ngl: a ? settings.ngl : Math.max(0, settings.ngl) })}
                autoNote="Automatic — VRAM-tiered, with a safety cap toward CPU-only for large models on shared-memory GPUs."
              />
              <SliderField
                label="Context Length"
                desc="Context window size in tokens (-c). Auto scales with detected VRAM and is capped down further for large models, since KV cache and model weights compete for the same memory."
                value={settings.ctx_size}
                min={512} max={131072} step={512}
                onChange={(v) => update('ctx_size', v)}
                auto={settings.ctx_size_auto}
                onAutoChange={(a) => updateFields({ ctx_size_auto: a })}
              />
              <SliderField
                label="CPU Threads"
                desc="Threads for llama-server (-t). Auto uses available parallelism, or whatever a launch script already configured. Has little effect when GPU Offload is high — most compute already runs on the GPU."
                value={settings.threads}
                min={1} max={128} step={1}
                onChange={(v) => update('threads', v)}
                auto={settings.threads_auto}
                onAutoChange={(a) => updateFields({ threads_auto: a })}
              />
              <SliderField
                label="Batch Size"
                desc="Prompt-eval batch size (-b). Larger values speed up prompt processing at the cost of more memory."
                value={settings.batch_size ?? 1024}
                min={128} max={8192} step={128}
                onChange={(v) => update('batch_size', v)}
                auto={settings.batch_size === null}
                onAutoChange={(a) => update('batch_size', a ? null : 1024)}
              />
              <SliderField
                label="Micro-batch Size"
                desc="Prompt-eval micro-batch size (-ub)."
                value={settings.ubatch_size ?? 512}
                min={32} max={2048} step={32}
                onChange={(v) => update('ubatch_size', v)}
                auto={settings.ubatch_size === null}
                onAutoChange={(a) => update('ubatch_size', a ? null : 512)}
              />
              <SelectField
                label="KV Cache Type"
                desc={settings.flash_attention
                  ? "Key/value cache quantization (-ctk/-ctv). q8_0 roughly halves cache memory vs f16 with negligible quality loss for agent/tool use."
                  : "Disabled — llama.cpp requires Flash Attention for a quantized KV cache. Using f16 (unquantized) while Flash Attention is off."}
                value={settings.flash_attention ? (settings.kv_cache_type ?? 'auto') : 'f16'}
                options={[
                  { value: 'auto', label: 'Auto (q8_0)' },
                  { value: 'f16', label: 'F16 (unquantized)' },
                  { value: 'q8_0', label: 'Q8_0' },
                  { value: 'q4_0', label: 'Q4_0' },
                ]}
                onChange={(v) => update('kv_cache_type', v === 'auto' ? null : v)}
                disabled={!settings.flash_attention}
              />
              <ToggleField
                label="Flash Attention"
                desc="Fused attention kernel — lower memory bandwidth on long context. Recommended on."
                checked={settings.flash_attention}
                onChange={(v) => update('flash_attention', v)}
                warning={!settings.flash_attention ? 'Flash Attention is off: KV cache quantization is unavailable while this is off.' : undefined}
              />
              <TriStateField
                label="Keep Model Locked in RAM (mlock)"
                desc="Pins model pages so the OS can't swap them out. Auto enables it only on hosts with plenty of RAM (>=24GB) — forcing it on a memory-tight host can starve everything else instead of just risking swap."
                value={settings.mlock}
                onChange={(v) => update('mlock', v)}
              />
              <TriStateField
                label="Disable Memory-Mapped Loading (no-mmap)"
                desc="Reads the whole model into memory upfront instead of mapping it and faulting pages in lazily. No measured benefit from forcing this on — off (mmap stays enabled) unless you've tested otherwise."
                value={settings.no_mmap}
                onChange={(v) => update('no_mmap', v)}
              />
                </>
              )}
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