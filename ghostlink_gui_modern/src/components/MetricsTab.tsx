import React, { useState, useEffect } from 'react';
import { RefreshCw } from 'lucide-react';
import { useAppStore, Metric } from '../store';

interface MetricsTabProps {
  api: any;
}

export const MetricsTab: React.FC<MetricsTabProps> = React.memo(({ api }) => {
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
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-xl font-bold text-white">Live Metrics Dashboard</h2>
        <button
          onClick={refreshMetrics}
          className={`p-2 rounded bg-slate-800 text-slate-300 hover:bg-slate-700 transition ${
            loading ? 'animate-spin' : ''
          }`}
        >
          <RefreshCw size={20} />
        </button>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
        <Gauge label="Throughput (Tokens/s)" value={metrics?.throughput || 0} max={1000} color="text-cyan-400" />
        <Gauge label="CPU Usage (%)" value={metrics?.cpu || 0} max={100} color="text-orange-400" />
        <Gauge label="Memory Usage (%)" value={metrics?.memory || 0} max={100} color="text-purple-400" />
        <Gauge label="GPU Usage (%)" value={metrics?.gpu || 0} max={100} color="text-green-400" />
        <Gauge label="Latency P50 (ms)" value={metrics?.latency_p50 || 0} max={50} color="text-yellow-400" />
        <Gauge label="Latency P95 (ms)" value={metrics?.latency_p95 || 0} max={100} color="text-red-400" />
      </div>

      <div className="bg-slate-800/50 p-6 rounded-lg border border-slate-700">
        <h3 className="text-lg font-semibold text-white mb-4">Cluster Health Summary</h3>
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
          <div className="flex items-center gap-3">
            <div className="w-3 h-3 rounded-full bg-green-400 animate-pulse" />
            <span className="text-slate-300">Discovery Protocol Active (UDP)</span>
          </div>
          <div className="flex items-center gap-3">
            <div className="w-3 h-3 rounded-full bg-green-400" />
            <span className="text-slate-300">P2P Mesh Fabric Connected</span>
          </div>
        </div>
      </div>
    </div>
  );
});

interface GaugeProps {
  label: string;
  value: number;
  max: number;
  color: string;
}

const Gauge: React.FC<GaugeProps> = ({ label, value, max, color }) => {
  const percentage = Math.min(100, (value / max) * 100);
  return (
    <div className="bg-slate-800 p-6 rounded-lg border border-slate-700 flex flex-col items-center">
      <div className="relative w-32 h-32">
        <svg className="w-full h-full transform -rotate-90">
          <circle
            cx="64"
            cy="64"
            r="58"
            stroke="currentColor"
            strokeWidth="8"
            fill="transparent"
            className="text-slate-700"
          />
          <circle
            cx="64"
            cy="64"
            r="58"
            stroke="currentColor"
            strokeWidth="8"
            fill="transparent"
            strokeDasharray={364.4}
            strokeDashoffset={364.4 - (364.4 * percentage) / 100}
            className={`${color} transition-all duration-500 ease-out`}
          />
        </svg>
        <div className="absolute inset-0 flex flex-col items-center justify-center">
          <span className="text-2xl font-bold text-white">{value.toFixed(1)}</span>
        </div>
      </div>
      <div className="mt-4 text-center">
        <p className="text-sm font-semibold text-slate-300">{label}</p>
        <div className="flex items-center gap-1 justify-center mt-2">
          <div
            className={`w-2 h-2 rounded-full ${
              percentage < 50 ? 'bg-green-400' : percentage < 80 ? 'bg-yellow-400' : 'bg-red-400'
            }`}
          />
          <span className="text-xs text-slate-400">
            {percentage < 50 ? 'Healthy' : percentage < 80 ? 'Caution' : 'Alert'}
          </span>
        </div>
      </div>
    </div>
  );
};
