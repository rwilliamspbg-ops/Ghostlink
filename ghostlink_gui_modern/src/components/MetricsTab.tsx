import React, { useMemo } from 'react';
import { RefreshCw, Activity, Cpu, Database, Zap, Clock, ShieldCheck, Server } from 'lucide-react';
import { MetricsHistoryPoint } from '../api';
import { useAppStore } from '../store';

export const MetricsTab: React.FC<{ api: any }> = React.memo(({ api }) => {
  const { metrics, setMetrics } = useAppStore();
  const [loading, setLoading] = React.useState(false);
  const [history, setHistory] = React.useState<MetricsHistoryPoint[]>([]);

  const refreshHistory = React.useCallback(async () => {
    const result = api.getMetricsHistory ? await api.getMetricsHistory() : { history: [] };
    if (!result.error && result.history) {
      setHistory(result.history);
    }
  }, [api]);

  React.useEffect(() => {
    refreshHistory();
  }, [refreshHistory]);

  // App.tsx already polls metrics every 5s — only manual refresh here.
  const refreshMetrics = async () => {
    setLoading(true);
    try {
      const result = await api.getMetrics();
      if (!result.error && result.metrics) {
        setMetrics(result.metrics);
      }
      await refreshHistory();
    } finally {
      setLoading(false);
    }
  };

  const throughputScale = useMemo(() => {
    const t = metrics?.throughput ?? 0;
    // Dynamic bar: 0–100 mapped against a soft ceiling near recent peak.
    const ceiling = Math.max(50, t * 1.25, 10);
    return Math.min(100, (t / ceiling) * 100);
  }, [metrics?.throughput]);

  const latP50Scale = useMemo(() => {
    const v = metrics?.latency_p50 ?? 0;
    // Lower is better; 500ms = full red bar.
    return Math.min(100, (v / 500) * 100);
  }, [metrics?.latency_p50]);

  const latP95Scale = useMemo(() => {
    const v = metrics?.latency_p95 ?? 0;
    return Math.min(100, (v / 1000) * 100);
  }, [metrics?.latency_p95]);

  const gpuLabel = metrics?.gpu_available
    ? `${(metrics?.gpu ?? 0).toFixed(0)}% utilized`
    : 'probe unavailable';

  const throughputSparkline = useMemo(() => buildSparkline(history, 'throughput', 220, 56), [history]);
  const latencySparkline = useMemo(() => buildSparkline(history, 'latency_p95', 220, 56), [history]);
  const historySummary = history.length > 0
    ? `${history.length} recent samples`
    : 'History will appear after metrics polling';

  return (
    <div className="flex flex-col h-full bg-slate-950">
      <div className="flex items-center justify-between px-6 py-4 border-b border-slate-900 sticky top-0 bg-slate-950/50 backdrop-blur-md z-10">
        <div>
          <h2 className="text-xl font-bold text-white">System Performance</h2>
          <p className="text-xs text-slate-500 mt-0.5">
            Live host + inference metrics
            {metrics?.inference_backend ? ` · ${metrics.inference_backend}` : ''}
            {typeof metrics?.samples === 'number' ? ` · ${metrics.samples} samples` : ''}
          </p>
        </div>
        <button
          onClick={refreshMetrics}
          className="p-2 rounded-lg hover:bg-slate-900 text-slate-400 hover:text-white transition"
          title="Refresh metrics"
        >
          <RefreshCw size={18} className={loading ? 'animate-spin' : ''} />
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        <div className="max-w-5xl mx-auto space-y-6">
          <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
            <StatCard
              label="Throughput"
              value={metrics?.throughput ?? 0}
              unit="tok/s"
              icon={Zap}
              color="text-cyan-400"
              bg="bg-cyan-500/10"
              progress={throughputScale}
            />
            <StatCard
              label="Latency p50"
              value={metrics?.latency_p50 ?? 0}
              unit="ms"
              icon={Clock}
              color="text-orange-400"
              bg="bg-orange-500/10"
              progress={latP50Scale}
              inverse
            />
            <StatCard
              label="Latency p95"
              value={metrics?.latency_p95 ?? 0}
              unit="ms"
              icon={Activity}
              color="text-red-400"
              bg="bg-red-500/10"
              progress={latP95Scale}
              inverse
            />
          </div>

          <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
            <GaugeCard label="CPU Utilization" value={metrics?.cpu ?? 0} icon={Cpu} color="text-emerald-400" />
            <GaugeCard label="Memory Usage" value={metrics?.memory ?? 0} icon={Database} color="text-purple-400" />
            <GaugeCard
              label="GPU Core Load"
              value={metrics?.gpu ?? 0}
              icon={ShieldCheck}
              color="text-blue-400"
              subtitle={gpuLabel}
            />
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            <HistoryCard
              title="Throughput Trend"
              subtitle={historySummary}
              value={`${(metrics?.throughput ?? 0).toFixed(1)} tok/s`}
              colorClass="text-cyan-400"
              path={throughputSparkline}
            />
            <HistoryCard
              title="Latency p95 Trend"
              subtitle={historySummary}
              value={`${(metrics?.latency_p95 ?? 0).toFixed(1)} ms`}
              colorClass="text-red-400"
              path={latencySparkline}
            />
          </div>

          <div className="bg-slate-900/50 border border-slate-800 rounded-3xl p-8 relative overflow-hidden">
            <div className="absolute top-0 right-0 p-8 opacity-5">
              <Zap size={120} />
            </div>
            <div className="relative z-10">
              <h3 className="text-lg font-bold text-white mb-6 flex items-center gap-2">
                <Activity size={20} className="text-blue-500" />
                Runtime Health
              </h3>
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-8">
                <div className="space-y-4">
                  <StatusRow
                    ok={!!metrics}
                    label={`API Server ${metrics ? 'Connected' : 'Offline'}`}
                  />
                  <StatusRow
                    ok={!!metrics?.gpu_available}
                    label={`GPU: ${gpuLabel}`}
                    warn={!metrics?.gpu_available}
                  />
                  <StatusRow
                    ok={(metrics?.active_nodes ?? 0) > 0}
                    label={`Active nodes: ${metrics?.active_nodes ?? 0}`}
                    icon
                  />
                </div>
                <div className="space-y-4">
                  <StatusRow
                    ok={(metrics?.cpu ?? 0) >= 0 && !!metrics}
                    label={`CPU: ${(metrics?.cpu ?? 0).toFixed(1)}%`}
                  />
                  <StatusRow
                    ok={(metrics?.memory ?? 0) > 0}
                    label={`Memory: ${(metrics?.memory ?? 0).toFixed(1)}%${
                      metrics?.used_memory_gb != null && metrics?.total_memory_gb
                        ? ` (${metrics.used_memory_gb.toFixed(1)}/${metrics.total_memory_gb.toFixed(1)} GB)`
                        : ''
                    }`}
                  />
                  <StatusRow
                    ok={!!metrics?.real_inference}
                    label={
                      metrics?.real_inference
                        ? 'Real inference active'
                        : 'Waiting for chat / simulated'
                    }
                    warn={!metrics?.real_inference}
                  />
                </div>
              </div>
              {(metrics?.total_vram_gb ?? 0) > 0 && (
                <p className="mt-6 text-xs text-slate-500 flex items-center gap-2">
                  <Server size={14} />
                  Detected VRAM pool: {metrics!.total_vram_gb!.toFixed(1)} GB
                  {typeof metrics?.uptime_s === 'number'
                    ? ` · uptime ${Math.floor(metrics.uptime_s / 60)}m`
                    : ''}
                </p>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
});

const StatusRow = ({
  ok,
  label,
  warn = false,
}: {
  ok: boolean;
  label: string;
  warn?: boolean;
  icon?: boolean;
}) => (
  <div className="flex items-center gap-3">
    <div
      className={`w-2 h-2 rounded-full ${
        ok
          ? 'bg-green-500 shadow-[0_0_8px_rgba(34,197,94,0.5)] animate-pulse'
          : warn
            ? 'bg-blue-500 shadow-[0_0_8px_rgba(59,130,246,0.5)]'
            : 'bg-slate-600'
      }`}
    />
    <span className="text-sm font-medium text-slate-300">{label}</span>
  </div>
);

function buildSparkline(
  history: MetricsHistoryPoint[],
  key: 'throughput' | 'latency_p95',
  width: number,
  height: number,
): string {
  if (history.length === 0) {
    return '';
  }

  const values = history.map((point) => Number(point[key]) || 0);
  const min = Math.min(...values);
  const max = Math.max(...values);
  const range = max - min || 1;

  return values
    .map((value, index) => {
      const x = history.length === 1 ? width / 2 : (index / (history.length - 1)) * width;
      const y = height - ((value - min) / range) * (height - 8) - 4;
      return `${x},${y}`;
    })
    .join(' ');
}

const HistoryCard = ({
  title,
  subtitle,
  value,
  colorClass,
  path,
}: {
  title: string;
  subtitle: string;
  value: string;
  colorClass: string;
  path: string;
}) => (
  <div className="bg-slate-900/50 border border-slate-800 rounded-3xl p-6">
    <div className="flex items-start justify-between gap-4 mb-4">
      <div>
        <p className="text-[10px] font-bold text-slate-500 uppercase tracking-widest">{title}</p>
        <p className="text-xs text-slate-500 mt-1">{subtitle}</p>
      </div>
      <p className={`text-lg font-black ${colorClass}`}>{value}</p>
    </div>
    <div className="h-16 rounded-2xl bg-slate-950/80 border border-slate-800 px-2 py-1">
      <svg viewBox="0 0 220 56" className="w-full h-full" role="img" aria-label={title}>
        {path ? (
          <polyline
            fill="none"
            stroke="currentColor"
            strokeWidth="3"
            strokeLinecap="round"
            strokeLinejoin="round"
            points={path}
            className={colorClass}
          />
        ) : (
          <text x="110" y="30" textAnchor="middle" className="fill-slate-600 text-[10px]">
            Waiting for samples
          </text>
        )}
      </svg>
    </div>
  </div>
);

const StatCard = ({
  label,
  value,
  unit,
  icon: Icon,
  color,
  bg,
  progress,
  inverse = false,
}: any) => (
  <div className="bg-slate-900/50 border border-slate-800 rounded-3xl p-6 hover:border-slate-700 transition-all group">
    <div className="flex items-center justify-between mb-4">
      <div className={`p-3 rounded-2xl ${bg} ${color}`}>
        <Icon size={24} />
      </div>
      <div className="text-right">
        <p className="text-[10px] font-bold text-slate-500 uppercase tracking-widest">{label}</p>
        <p className={`text-2xl font-black ${color}`}>
          {Number(value).toFixed(1)}{' '}
          <span className="text-xs font-medium opacity-50">{unit}</span>
        </p>
      </div>
    </div>
    <div className="h-1.5 bg-slate-800 rounded-full overflow-hidden">
      <div
        className={`h-full transition-all duration-1000 ${
          inverse
            ? progress < 30
              ? 'bg-green-500'
              : progress < 70
                ? 'bg-orange-500'
                : 'bg-red-500'
            : progress > 70
              ? 'bg-green-500'
              : progress > 30
                ? 'bg-orange-500'
                : 'bg-cyan-500'
        }`}
        style={{ width: `${Math.min(100, Math.max(2, progress || 0))}%` }}
      />
    </div>
  </div>
);

const GaugeCard = ({
  label,
  value,
  icon: Icon,
  color,
  subtitle,
}: {
  label: string;
  value: number;
  icon: any;
  color: string;
  subtitle?: string;
}) => (
  <div className="bg-slate-900/50 border border-slate-800 rounded-3xl p-6 flex flex-col items-center text-center">
    <div className="relative w-24 h-24 mb-4">
      <svg className="w-full h-full transform -rotate-90">
        <circle
          cx="48"
          cy="48"
          r="42"
          stroke="currentColor"
          strokeWidth="6"
          fill="transparent"
          className="text-slate-800"
        />
        <circle
          cx="48"
          cy="48"
          r="42"
          stroke="currentColor"
          strokeWidth="6"
          fill="transparent"
          strokeDasharray={263.8}
          strokeDashoffset={263.8 - (263.8 * Math.min(100, Math.max(0, value))) / 100}
          className={`${color} transition-all duration-1000 ease-out`}
        />
      </svg>
      <div className="absolute inset-0 flex items-center justify-center">
        <Icon size={20} className={color} />
      </div>
    </div>
    <p className="text-xs font-bold text-slate-300 mb-1">{label}</p>
    <p className="text-xl font-black text-white">{Number(value).toFixed(1)}%</p>
    {subtitle && <p className="text-[10px] text-slate-500 mt-1">{subtitle}</p>}
  </div>
);
