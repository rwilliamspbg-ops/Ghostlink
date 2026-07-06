import React, { useState, useEffect } from 'react';
import { RefreshCw } from 'lucide-react';
import { useAppStore, Metric } from '../store';

interface MetricsTabProps {
  api: any;
}

export const MetricsTab: React.FC<MetricsTabProps> = ({ api }) => {
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
  }, []);

  return (
    <div className="space-y-6">
      {/* Refresh Button */}
      <div className="flex gap-2">
        <button
          onClick={refreshMetrics}
          disabled={loading}
          className="flex items-center gap-2 px-4 py-2 bg-slate-700 hover:bg-slate-600 disabled:bg-slate-800 text-white rounded transition"
        >
          <RefreshCw size={16} className={loading ? 'animate-spin' : ''} />
          Refresh
        </button>
      </div>

      {/* Digital Gauges Grid */}
      {metrics && (
        <div className="grid grid-cols-2 md:grid-cols-3 gap-6">
          <DigitalGauge
            label="Throughput"
            value={metrics.throughput || 0}
            unit="req/s"
            max={100}
            color="from-cyan-500 to-blue-600"
          />
          <DigitalGauge
            label="CPU Usage"
            value={metrics.cpu || 0}
            unit="%"
            max={100}
            color="from-orange-500 to-red-600"
          />
          <DigitalGauge
            label="Memory"
            value={metrics.memory || 0}
            unit="%"
            max={100}
            color="from-purple-500 to-pink-600"
          />
          <DigitalGauge
            label="GPU Usage"
            value={metrics.gpu || 0}
            unit="%"
            max={100}
            color="from-green-500 to-emerald-600"
          />
          <DigitalGauge
            label="Latency P50"
            value={metrics.latency_p50 || 0}
            unit="ms"
            max={500}
            color="from-yellow-500 to-orange-600"
          />
          <DigitalGauge
            label="Latency P95"
            value={metrics.latency_p95 || 0}
            unit="ms"
            max={1000}
            color="from-red-500 to-rose-600"
          />
        </div>
      )}

      {/* Raw JSON */}
      <div className="bg-slate-900 rounded p-4 border border-slate-700">
        <h3 className="text-sm font-semibold text-slate-200 mb-3">Raw Data</h3>
        <pre className="text-slate-300 text-sm overflow-x-auto font-mono bg-slate-800 p-3 rounded max-h-64">
          {metrics ? JSON.stringify(metrics, null, 2) : '{}'}
        </pre>
      </div>
    </div>
  );
};

interface DigitalGaugeProps {
  label: string;
  value: number;
  unit: string;
  max: number;
  color: string;
}

const DigitalGauge: React.FC<DigitalGaugeProps> = ({ label, value, unit, max, color }) => {
  const percentage = Math.min((value / max) * 100, 100);
  const angle = (percentage / 100) * 270 - 135; // -135 to 135 degrees

  const getGaugeColor = () => {
    if (percentage < 50) return 'from-green-400 to-green-600';
    if (percentage < 80) return 'from-yellow-400 to-yellow-600';
    return 'from-red-400 to-red-600';
  };

  return (
    <div className="flex flex-col items-center">
      <div className="relative w-40 h-40 mb-4">
        {/* Gauge background */}
        <svg
          className="absolute inset-0 w-full h-full"
          viewBox="0 0 100 100"
          style={{
            filter: 'drop-shadow(0 0 10px rgba(0,0,0,0.5))',
          }}
        >
          {/* Background arc */}
          <defs>
            <linearGradient id={`gauge-${label}`} x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="#334155" />
              <stop offset="100%" stopColor="#0f172a" />
            </linearGradient>
          </defs>

          {/* Outer circle */}
          <circle cx="50" cy="50" r="48" fill="url(#gauge-bg)" stroke="#1e293b" strokeWidth="1" />

          {/* Background gauge arc */}
          <path
            d="M 20 80 A 30 30 0 0 1 80 80"
            fill="none"
            stroke="#334155"
            strokeWidth="6"
            strokeLinecap="round"
          />

          {/* Active gauge arc */}
          <path
            d="M 20 80 A 30 30 0 0 1 80 80"
            fill="none"
            stroke={percentage < 50 ? '#22c55e' : percentage < 80 ? '#eab308' : '#ef4444'}
            strokeWidth="6"
            strokeLinecap="round"
            strokeDasharray={`${(percentage / 100) * 94.25} 94.25`}
            style={{ transition: 'stroke-dasharray 0.3s ease' }}
          />

          {/* Needle */}
          <g transform={`rotate(${angle} 50 50)`}>
            <line
              x1="50"
              y1="50"
              x2="50"
              y2="22"
              stroke="#e2e8f0"
              strokeWidth="2"
              strokeLinecap="round"
            />
            <circle cx="50" cy="50" r="3" fill="#e2e8f0" />
          </g>

          {/* Center dot */}
          <circle cx="50" cy="50" r="4" fill="#1e293b" stroke="#64748b" strokeWidth="1" />
        </svg>

        {/* Digital display */}
        <div className="absolute inset-0 flex flex-col items-center justify-center">
          <div className="text-2xl font-bold text-slate-100 font-mono">
            {value.toFixed(1)}
          </div>
          <div className="text-xs text-slate-400">{unit}</div>
        </div>
      </div>

      {/* Label and status */}
      <div className="text-center">
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
