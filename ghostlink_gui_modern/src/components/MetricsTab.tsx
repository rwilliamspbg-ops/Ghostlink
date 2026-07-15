import React, { useState, useEffect } from 'react';
import { RefreshCw, Activity, Cpu, Database, Zap, Clock, ShieldCheck } from 'lucide-react';
import { useAppStore } from '../store';

export const MetricsTab: React.FC<{ api: any }> = React.memo(({ api }) => {
  const { metrics, setMetrics } = useAppStore();
  const [loading, setLoading] = useState(false);

  const refreshMetrics = async () => {
    setLoading(true);
    const result = await api.getMetrics();
    if (!result.error) {
      setMetrics(result.metrics);
    }
    setLoading(false);
  };

  useEffect(() => {
    refreshMetrics();
    const interval = setInterval(refreshMetrics, 5000);
    return () => clearInterval(interval);
  }, [api]);

  return (
    <div className="flex flex-col h-full bg-slate-950">
      <div className="flex items-center justify-between px-6 py-4 border-b border-slate-900 sticky top-0 bg-slate-950/50 backdrop-blur-md z-10">
        <h2 className="text-xl font-bold text-white">System Performance</h2>
        <button
          onClick={refreshMetrics}
          className="p-2 rounded-lg hover:bg-slate-900 text-slate-400 hover:text-white transition"
        >
          <RefreshCw size={18} className={loading ? 'animate-spin' : ''} />
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        <div className="max-w-5xl mx-auto space-y-6">
          {/* Main Stats Grid */}
          <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
            <StatCard
                label="Total Throughput"
                value={metrics?.throughput || 0}
                unit="Tokens/sec"
                icon={Zap}
                color="text-cyan-400"
                bg="bg-cyan-500/10"
                progress={(metrics?.throughput || 0) / 10}
            />
            <StatCard
                label="System Latency"
                value={metrics?.latency_p50 || 0}
                unit="ms (p50)"
                icon={Clock}
                color="text-orange-400"
                bg="bg-orange-500/10"
                progress={(metrics?.latency_p50 || 0) * 2}
                inverse
            />
            <StatCard
                label="Peak Latency"
                value={metrics?.latency_p95 || 0}
                unit="ms (p95)"
                icon={Activity}
                color="text-red-400"
                bg="bg-red-500/10"
                progress={(metrics?.latency_p95 || 0)}
                inverse
            />
          </div>

          {/* Resource Gauges */}
          <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
            <GaugeCard label="CPU Utilization" value={metrics?.cpu || 0} icon={Cpu} color="text-emerald-400" />
            <GaugeCard label="Memory Usage" value={metrics?.memory || 0} icon={Database} color="text-purple-400" />
            <GaugeCard label="GPU Core Load" value={metrics?.gpu || 0} icon={ShieldCheck} color="text-blue-400" />
          </div>

          {/* Fabric Status — now driven by real workers/metrics */}
          <div className="bg-slate-900/50 border border-slate-800 rounded-3xl p-8 relative overflow-hidden">
            <div className="absolute top-0 right-0 p-8 opacity-5">
                <Zap size={120} />
            </div>
            <div className="relative z-10">
                <h3 className="text-lg font-bold text-white mb-6 flex items-center gap-2">
                    <Activity size={20} className="text-blue-500" />
                    Distributed Fabric Health
                </h3>
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-8">
                    <div className="space-y-4">
                        <div className="flex items-center gap-3">
                            <div className={`w-2 h-2 rounded-full shadow-[0_0_8px_rgba(34,197,94,0.5)] ${metrics ? 'bg-green-500 animate-pulse' : 'bg-slate-600'}`} />
                            <span className="text-sm font-medium text-slate-300">API Server {metrics ? 'Connected' : 'Offline'}</span>
                        </div>
                        <div className="flex items-center gap-3">
                            <div className={`w-2 h-2 rounded-full shadow-[0_0_8px_rgba(34,197,94,0.5)] ${(metrics?.gpu ?? 0) > 0 ? 'bg-green-500' : 'bg-blue-500'}`} />
                            <span className="text-sm font-medium text-slate-300">GPU Acceleration: {metrics?.gpu.toFixed(0) ?? '?'}% utilized</span>
                        </div>
                    </div>
                    <div className="space-y-4">
                        <div className="flex items-center gap-3">
                            <div className={`w-2 h-2 rounded-full shadow-[0_0_8px_rgba(34,197,94,0.5)] ${metrics && metrics.cpu > 0 ? 'bg-green-500' : 'bg-slate-600'}`} />
                            <span className="text-sm font-medium text-slate-300">CPU Load: {metrics?.cpu.toFixed(1) ?? '?'}%</span>
                        </div>
                        <div className="flex items-center gap-3">
                            <div className={`w-2 h-2 rounded-full shadow-[0_0_8px_rgba(59,130,246,0.5)] ${metrics && metrics.memory > 0 ? 'bg-blue-500' : 'bg-slate-600'}`} />
                            <span className="text-sm font-medium text-slate-300">Memory: {metrics?.memory.toFixed(1) ?? '?'}% used</span>
                        </div>
                    </div>
                </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
});

const StatCard = ({ label, value, unit, icon: Icon, color, bg, progress, inverse = false }: any) => (
  <div className="bg-slate-900/50 border border-slate-800 rounded-3xl p-6 hover:border-slate-700 transition-all group">
    <div className="flex items-center justify-between mb-4">
      <div className={`p-3 rounded-2xl ${bg} ${color}`}>
        <Icon size={24} />
      </div>
      <div className="text-right">
        <p className="text-[10px] font-bold text-slate-500 uppercase tracking-widest">{label}</p>
        <p className={`text-2xl font-black ${color}`}>{value.toFixed(1)} <span className="text-xs font-medium opacity-50">{unit}</span></p>
      </div>
    </div>
    <div className="h-1.5 bg-slate-800 rounded-full overflow-hidden">
        <div
            className={`h-full transition-all duration-1000 ${
                inverse
                    ? (progress < 30 ? 'bg-green-500' : progress < 70 ? 'bg-orange-500' : 'bg-red-500')
                    : (progress > 70 ? 'bg-green-500' : progress > 30 ? 'bg-orange-500' : 'bg-red-500')
            }`}
            style={{ width: `${Math.min(100, Math.max(5, progress))}%` }}
        />
    </div>
  </div>
);

const GaugeCard = ({ label, value, icon: Icon, color }: any) => (
    <div className="bg-slate-900/50 border border-slate-800 rounded-3xl p-6 flex flex-col items-center text-center">
        <div className="relative w-24 h-24 mb-4">
            <svg className="w-full h-full transform -rotate-90">
                <circle cx="48" cy="48" r="42" stroke="currentColor" strokeWidth="6" fill="transparent" className="text-slate-800" />
                <circle
                    cx="48" cy="48" r="42" stroke="currentColor" strokeWidth="6" fill="transparent"
                    strokeDasharray={263.8}
                    strokeDashoffset={263.8 - (263.8 * value) / 100}
                    className={`${color} transition-all duration-1000 ease-out`}
                />
            </svg>
            <div className="absolute inset-0 flex items-center justify-center">
                <Icon size={20} className={color} />
            </div>
        </div>
        <p className="text-xs font-bold text-slate-300 mb-1">{label}</p>
        <p className="text-xl font-black text-white">{value.toFixed(1)}%</p>
    </div>
  );
