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
  Monitor,
  Cpu as GpuIcon,
} from 'lucide-react';
import { Settings as SettingsType } from '../store';

export const SettingsTab: React.FC<{ api: any }> = ({ api }) => {
  const [settings, setSettings] = useState<SettingsType | null>(null);
  const [loading, setLoading] = useState(true);
  const [runtimes, setRuntimes] = useState<any[]>([]);
  const [detectedGpuName, setDetectedGpuName] = useState<string>('');
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState('');

  const loadSettings = useCallback(async () => {
    setLoading(true);
    const [result, runtimeResult] = await Promise.all([
      api.getSettings(),
      api.getRuntimes().catch(() => ({ available_runtimes: [] })),
    ]);
    if (result.settings && !result.error) {
      setSettings(result.settings);
    } else {
      setError(result.error || 'Failed to load settings');
    }
    if (runtimeResult?.available_runtimes) {
      setRuntimes(runtimeResult.available_runtimes);
      const gpuRuntime = runtimeResult.available_runtimes.find(
        (r: any) => r.runtime?.includes('NVIDIA') || r.runtime?.includes('AMD') || r.runtime?.includes('ROCm')
      );
      if (gpuRuntime) {
        setDetectedGpuName(gpuRuntime.runtime);
      }
    }
    setLoading(false);
  }, [api]);

  useEffect(() => {
    loadSettings();
  }, [loadSettings]);

  const update = (key: string, value: any) => {
    if (!settings) return;
    setSettings({ ...settings, [key]: value });
  };

  const handleSave = async () => {
    if (!settings) return;
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
  }) => (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between">
        <label className="text-sm font-medium text-slate-300">{label}</label>
        <span className="text-xs font-mono text-blue-400 bg-blue-500/10 px-2 py-0.5 rounded-md">{value}{unit}</span>
      </div>
      {desc && <p className="text-[10px] text-slate-500">{desc}</p>}
      <input
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

  const SelectField = ({ label, desc, value, options, onChange }: {
    label: string; desc?: string; value: string; options: { value: string; label: string }[]; onChange: (v: string) => void;
  }) => (
    <div className="space-y-1.5">
      <label className="text-sm font-medium text-slate-300">{label}</label>
      {desc && <p className="text-[10px] text-slate-500">{desc}</p>}
      <select
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

  const InputField = ({ label, desc, value, onChange, type = 'text', placeholder }: {
    label: string; desc?: string; value: string; onChange: (v: string) => void; type?: string; placeholder?: string;
  }) => (
    <div className="space-y-1.5">
      <label className="text-sm font-medium text-slate-300">{label}</label>
      {desc && <p className="text-[10px] text-slate-500">{desc}</p>}
      <input
        type={type}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="w-full bg-slate-800 border border-slate-700 rounded-xl px-3 py-2.5 text-sm text-slate-200 focus:outline-none focus:ring-2 focus:ring-blue-500/50 focus:border-blue-500/50 font-mono"
      />
    </div>
  );

  return (
    <div className="flex flex-col h-full bg-slate-950">
      <div className="flex items-center justify-between px-6 py-4 border-b border-slate-900 sticky top-0 bg-slate-950/50 backdrop-blur-md z-10">
        <div className="flex items-center gap-3">
          <Settings size={20} className="text-blue-400" />
          <h2 className="text-xl font-bold text-white">Runtime Settings</h2>
        </div>
        <div className="flex items-center gap-2">
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
          {/* Hardware Detection Card */}
          {runtimes.length > 0 && (
            <div className="bg-slate-900/50 border border-slate-800 rounded-3xl p-6">
              <div className="flex items-center gap-3 mb-4">
                <div className="p-3 bg-green-500/10 rounded-2xl text-green-400">
                  <Monitor size={24} />
                </div>
                <div>
                  <h3 className="font-bold text-slate-100">Detected Hardware</h3>
                  <p className="text-xs text-slate-500">Available runtimes on this system</p>
                </div>
              </div>
              <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
                {runtimes.map((rt, i) => {
                  const isGpu = rt.runtime?.includes('NVIDIA') || rt.runtime?.includes('AMD') || rt.runtime?.includes('ROCm');
                  const isCpu = rt.runtime?.includes('CPU');
                  const isNpu = rt.runtime?.includes('NPU');
                  return (
                    <div key={i} className={`p-3 rounded-2xl border ${
                      rt.is_available
                        ? isGpu ? 'bg-green-500/5 border-green-500/20' : isNpu ? 'bg-purple-500/5 border-purple-500/20' : 'bg-slate-800/50 border-slate-700'
                        : 'bg-slate-800/20 border-slate-800/30'
                    }`}>
                      <div className="flex items-center gap-2 mb-1">
                        {isGpu ? <GpuIcon size={14} className="text-green-400" /> : isCpu ? <Cpu size={14} className="text-slate-400" /> : <Zap size={14} className="text-purple-400" />}
                        <span className={`text-xs font-bold ${rt.is_available ? (isGpu ? 'text-green-400' : 'text-slate-300') : 'text-slate-600'}`}>
                          {rt.is_available ? 'Available' : 'Unavailable'}
                        </span>
                      </div>
                      <p className="text-sm font-mono text-slate-200 truncate" title={rt.runtime}>
                        {rt.runtime}
                      </p>
                      {rt.memory_gb && <p className="text-[10px] text-slate-500">{rt.memory_gb.toFixed(1)} GB</p>}
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            {/* Runtime Section */}
            <Section title="Inference Runtime" icon={Cpu}>
              <SelectField
                label="Backend"
                desc="Inference engine backend"
                value={settings.inference_backend}
                options={[
                  { value: 'native', label: 'Native (llama.cpp)' },
                  { value: 'ollama', label: 'Ollama' },
                ]}
                onChange={(v) => update('inference_backend', v)}
              />
              {settings.inference_backend === 'native' && (
                <SelectField
                  label="Native Engine"
                  desc="llama.cpp execution mode"
                  value={settings.native_engine}
                  options={[
                    { value: 'llama_server', label: 'llama-server (HTTP)' },
                    { value: 'llama_cpp', label: 'llama-cli (direct)' },
                    { value: 'simulated', label: 'Simulated (debug)' },
                  ]}
                  onChange={(v) => update('native_engine', v)}
                />
              )}
              <SliderField
                label="GPU Layers (NGL)"
                desc="Number of layers offloaded to GPU (-1 = all, 0 = CPU)"
                value={settings.ngl}
                min={-1} max={200} step={1}
                onChange={(v) => update('ngl', v)}
              />
              <SliderField
                label="CPU Threads"
                desc="Number of threads for inference"
                value={settings.threads}
                min={1} max={32} step={1}
                onChange={(v) => update('threads', v)}
              />
              <SliderField
                label="Context Size"
                desc="Maximum context window (tokens)"
                value={settings.ctx_size}
                min={512} max={32768} step={512}
                onChange={(v) => update('ctx_size', v)}
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
            <Section title="Model Configuration" icon={Server}>
              <InputField
                label="Model Path"
                desc="Path to GGUF model file"
                value={settings.model_path}
                onChange={(v) => update('model_path', v)}
                placeholder="/tmp/ghostlink-models/model.gguf"
              />
              <InputField
                label="llama-server URL"
                desc="llama.cpp server endpoint"
                value={settings.llama_server_url}
                onChange={(v) => update('llama_server_url', v)}
                placeholder="http://127.0.0.1:8080/completion"
              />
              <SliderField
                label="llama-server Port"
                desc="Port for the native inference server"
                value={settings.llama_port}
                min={1024} max={65535} step={1}
                onChange={(v) => update('llama_port', v)}
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