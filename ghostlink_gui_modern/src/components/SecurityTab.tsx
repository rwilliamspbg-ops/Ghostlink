import React, { useState, useEffect } from 'react';
import { Lock, Unlock, Zap, Shield, Clock, CheckCircle } from 'lucide-react';

interface SecurityTabProps {
  api: any;
}

export const SecurityTab: React.FC<SecurityTabProps> = ({ api }) => {
  const [jwtStatus, setJwtStatus] = useState<'active' | 'expired'>('active');
  const [pqcEnabled, setPqcEnabled] = useState(false);
  const [jwtRefreshTime, setJwtRefreshTime] = useState<number>(0);
  const [pqcEnabledTime, setPqcEnabledTime] = useState<number>(0);
  const [logs, setLogs] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    // Initialize vault status
    setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] Vault initialized`]);

    // Simulate JWT expiration countdown
    const jwtInterval = setInterval(() => {
      setJwtRefreshTime((prev) => {
        if (prev <= 0) {
          setJwtStatus('expired');
          setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] ⚠️  JWT token expired`]);
          return 3600;
        }
        return prev - 1;
      });
    }, 1000);

    return () => clearInterval(jwtInterval);
  }, []);

  const handleRefreshJWT = async () => {
    setLoading(true);
    const result = await api.refreshJWT();
    if (result.success) {
      setJwtStatus('active');
      setJwtRefreshTime(3600);
      setLogs((prev) => [
        ...prev,
        `[${new Date().toLocaleTimeString()}] ✓ JWT token refreshed successfully`,
      ]);
    } else {
      setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] ✗ JWT refresh failed: ${result.error}`]);
    }
    setLoading(false);
  };

  const handleEnablePQC = async () => {
    setLoading(true);
    const result = await api.enablePQC();
    if (result.success) {
      setPqcEnabled(true);
      setPqcEnabledTime(Date.now());
      setLogs((prev) => [
        ...prev,
        `[${new Date().toLocaleTimeString()}] ✓ Post-Quantum Cryptography enabled`,
      ]);
    } else {
      setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] ✗ PQC enable failed: ${result.error}`]);
    }
    setLoading(false);
  };

  const formatTime = (seconds: number) => {
    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    const secs = seconds % 60;
    return `${hours}h ${minutes}m ${secs}s`;
  };

  return (
    <div className="space-y-6">
      {/* Vault Container */}
      <div className="bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 rounded-lg p-6 border border-slate-700 shadow-2xl">
        {/* Vault Header */}
        <div className="flex items-center gap-3 mb-6">
          <div className="p-3 bg-slate-800 rounded-lg">
            <Shield className="text-amber-400" size={32} />
          </div>
          <div>
            <h2 className="text-2xl font-bold text-slate-100">Security Vault</h2>
            <p className="text-sm text-slate-400">Cryptographic Authentication Management</p>
          </div>
        </div>

        {/* Status Grid */}
        <div className="grid md:grid-cols-2 gap-4 mb-6">
          {/* JWT Status */}
          <div
            className={`rounded-lg p-4 border-2 transition-all ${
              jwtStatus === 'active'
                ? 'bg-emerald-950 border-emerald-600 shadow-lg shadow-emerald-600/20'
                : 'bg-red-950 border-red-600 shadow-lg shadow-red-600/20'
            }`}
          >
            <div className="flex items-start justify-between mb-3">
              <div>
                <p className="text-sm text-slate-300 mb-1">JWT Token Status</p>
                <p
                  className={`text-2xl font-bold ${
                    jwtStatus === 'active' ? 'text-emerald-400' : 'text-red-400'
                  }`}
                >
                  {jwtStatus === 'active' ? '🔓 ACTIVE' : '🔒 EXPIRED'}
                </p>
              </div>
              {jwtStatus === 'active' ? (
                <Unlock className="text-emerald-400" size={28} />
              ) : (
                <Lock className="text-red-400" size={28} />
              )}
            </div>

            <div className="space-y-2 mb-4">
              <div className="flex items-center gap-2 text-sm">
                <Clock size={14} className="text-slate-400" />
                <span className="text-slate-300">
                  Expires in: <span className="font-mono font-bold">{formatTime(jwtRefreshTime)}</span>
                </span>
              </div>
            </div>

            <button
              onClick={handleRefreshJWT}
              disabled={loading || jwtStatus === 'active'}
              className={`w-full py-2 px-4 rounded font-semibold transition ${
                jwtStatus === 'active' && !loading
                  ? 'bg-slate-700 text-slate-400 cursor-not-allowed'
                  : 'bg-emerald-600 hover:bg-emerald-700 text-white'
              }`}
            >
              {loading ? 'Refreshing...' : 'Refresh Token'}
            </button>
          </div>

          {/* PQC Status */}
          <div
            className={`rounded-lg p-4 border-2 transition-all ${
              pqcEnabled
                ? 'bg-purple-950 border-purple-600 shadow-lg shadow-purple-600/20'
                : 'bg-slate-800 border-slate-700'
            }`}
          >
            <div className="flex items-start justify-between mb-3">
              <div>
                <p className="text-sm text-slate-300 mb-1">Post-Quantum Cryptography</p>
                <p className={`text-2xl font-bold ${pqcEnabled ? 'text-purple-400' : 'text-slate-400'}`}>
                  {pqcEnabled ? '🔐 ENABLED' : '⚙️ DISABLED'}
                </p>
              </div>
              <Zap className={pqcEnabled ? 'text-purple-400' : 'text-slate-600'} size={28} />
            </div>

            <div className="space-y-2 mb-4">
              {pqcEnabled && (
                <div className="flex items-center gap-2 text-sm text-purple-300">
                  <CheckCircle size={14} />
                  <span>
                    Enabled: <span className="font-mono">{new Date(pqcEnabledTime).toLocaleTimeString()}</span>
                  </span>
                </div>
              )}
            </div>

            <button
              onClick={handleEnablePQC}
              disabled={loading || pqcEnabled}
              className={`w-full py-2 px-4 rounded font-semibold transition ${
                pqcEnabled && !loading
                  ? 'bg-slate-700 text-slate-400 cursor-not-allowed'
                  : 'bg-purple-600 hover:bg-purple-700 text-white'
              }`}
            >
              {loading ? 'Enabling...' : pqcEnabled ? 'Enabled' : 'Enable PQC'}
            </button>
          </div>
        </div>

        {/* Security Level Indicator */}
        <div className="mb-6 p-4 bg-slate-800 rounded-lg border border-slate-700">
          <p className="text-xs text-slate-400 mb-2">Overall Security Level</p>
          <div className="flex items-center gap-3">
            <div className="flex-1 h-3 bg-slate-700 rounded-full overflow-hidden">
              <div
                className={`h-full transition-all ${
                  jwtStatus === 'active' && pqcEnabled ? 'bg-emerald-500' : 'bg-yellow-500'
                }`}
                style={{
                  width: jwtStatus === 'active' && pqcEnabled ? '100%' : '60%',
                }}
              />
            </div>
            <span className="text-sm font-semibold text-slate-200 min-w-20">
              {jwtStatus === 'active' && pqcEnabled ? '🟢 Maximum' : '🟡 Standard'}
            </span>
          </div>
        </div>
      </div>

      {/* Audit Log */}
      <div className="bg-slate-900 rounded p-4 border border-slate-700">
        <h3 className="text-sm font-semibold text-slate-200 mb-3 flex items-center gap-2">
          <Lock size={16} />
          Security Audit Log
        </h3>
        <div className="h-64 overflow-y-auto bg-slate-800 rounded p-3 space-y-1 font-mono text-xs">
          {logs.length === 0 ? (
            <p className="text-slate-500 italic">No security events</p>
          ) : (
            logs.map((log, idx) => (
              <p key={idx} className="text-slate-300">
                {log}
              </p>
            ))
          )}
        </div>
      </div>

      {/* Security Recommendations */}
      <div className="grid md:grid-cols-2 gap-4">
        <div className="bg-slate-900 rounded p-4 border border-slate-700">
          <h4 className="font-semibold text-slate-200 mb-2">Recommendations</h4>
          <ul className="space-y-1 text-xs text-slate-400">
            <li>✓ Refresh JWT tokens regularly (every hour)</li>
            <li>✓ Enable PQC for quantum-resistant encryption</li>
            <li>✓ Monitor security logs for anomalies</li>
            <li>✓ Use strong credentials in production</li>
          </ul>
        </div>

        <div className="bg-slate-900 rounded p-4 border border-slate-700">
          <h4 className="font-semibold text-slate-200 mb-2">Status Summary</h4>
          <ul className="space-y-2 text-xs">
            <li className="flex justify-between">
              <span className="text-slate-400">JWT Status:</span>
              <span className={jwtStatus === 'active' ? 'text-emerald-400' : 'text-red-400'}>
                {jwtStatus}
              </span>
            </li>
            <li className="flex justify-between">
              <span className="text-slate-400">PQC Status:</span>
              <span className={pqcEnabled ? 'text-purple-400' : 'text-slate-500'}>{pqcEnabled ? 'Enabled' : 'Disabled'}</span>
            </li>
            <li className="flex justify-between">
              <span className="text-slate-400">Last Update:</span>
              <span className="text-slate-300">{new Date().toLocaleTimeString()}</span>
            </li>
          </ul>
        </div>
      </div>
    </div>
  );
};
