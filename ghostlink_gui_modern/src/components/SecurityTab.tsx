import React, { useState, useEffect } from 'react';
import { Shield, Key, RefreshCw, AlertTriangle, Eye, EyeOff, CheckCircle2 } from 'lucide-react';

interface AuditEntry {
  event: string;
  status: string;
  ip: string;
  time: string;
  detail?: string;
}

export const SecurityTab: React.FC<{ api: any }> = ({ api }) => {
  const [loading, setLoading] = useState(false);
  const [showToken, setShowShowToken] = useState(false);
  const [pqcEnabled, setPqcEnabled] = useState(false);
  const [token, setToken] = useState('eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...');
  const [auditLog, setAuditLog] = useState<AuditEntry[]>([]);

  useEffect(() => {
    const fetchPqcState = async () => {
      const result = await api.getPQCState();
      if (result.enabled !== undefined) setPqcEnabled(result.enabled);
    };
    const fetchAuditLog = async () => {
      const result = await api.getAuditLog();
      if (result.entries) {
        setAuditLog(result.entries.map((e: any) => ({
          ...e,
          time: e.time ? new Date(e.time).toLocaleString() : e.time,
        })));
      }
    };
    fetchPqcState();
    fetchAuditLog();
    const interval = setInterval(fetchAuditLog, 30000);
    return () => clearInterval(interval);
  }, [api]);

  const handleRefresh = async () => {
    setLoading(true);
    try {
      const result = await api.refreshJWT();
      if (result.success) setToken(result.data?.token || token);
    } finally {
      setLoading(false);
    }
  };

  const handlePqc = async () => {
    setLoading(true);
    try {
      const result = await api.enablePQC();
      if (result.success) setPqcEnabled(true);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex flex-col h-full bg-slate-950">
      <div className="flex items-center justify-between px-6 py-4 border-b border-slate-900 sticky top-0 bg-slate-950/50 backdrop-blur-md z-10">
        <h2 className="text-xl font-bold text-white">Security & Access</h2>
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        <div className="max-w-5xl mx-auto space-y-6">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            {/* JWT Section */}
            <div className="bg-slate-900/50 border border-slate-800 rounded-3xl p-6 space-y-6">
              <div className="flex items-center gap-3">
                <div className="p-3 bg-blue-500/10 rounded-2xl text-blue-400">
                  <Key size={24} />
                </div>
                <div>
                  <h3 className="font-bold text-slate-100">Access Token</h3>
                  <p className="text-xs text-slate-500">JWT Authentication for API access</p>
                </div>
              </div>

              <div className="bg-slate-950 border border-slate-800 rounded-2xl p-4 relative group">
                <div className="flex items-center justify-between mb-2">
                  <span className="text-[10px] font-bold text-slate-500 uppercase tracking-widest">Active Session Token</span>
                  <button
                    onClick={() => setShowShowToken(!showToken)}
                    className="text-slate-500 hover:text-white transition"
                    aria-label={showToken ? 'Hide token' : 'Show token'}
                    aria-pressed={showToken}
                  >
                    {showToken ? <EyeOff size={14} aria-hidden="true" /> : <Eye size={14} aria-hidden="true" />}
                  </button>
                </div>
                <div className="font-mono text-xs break-all text-slate-400 pr-8">
                  {showToken ? token : '••••••••••••••••••••••••••••••••••••••••••••••••'}
                </div>
              </div>

              <button
                onClick={handleRefresh}
                disabled={loading}
                className="w-full flex items-center justify-center gap-2 py-3 bg-slate-800 hover:bg-slate-700 text-white rounded-xl text-sm font-bold transition"
              >
                <RefreshCw size={16} className={loading ? 'animate-spin' : ''} aria-hidden="true" />
                Refresh Token
              </button>
            </div>

            {/* PQC Section */}
            <div className="bg-slate-900/50 border border-slate-800 rounded-3xl p-6 space-y-6">
              <div className="flex items-center gap-3">
                <div className="p-3 bg-purple-500/10 rounded-2xl text-purple-400">
                  <Shield size={24} />
                </div>
                <div>
                  <h3 className="font-bold text-slate-100">Quantum Hardening</h3>
                  <p className="text-xs text-slate-500">Post-Quantum Cryptography (PQC)</p>
                </div>
              </div>

              <div className={`p-4 rounded-2xl border transition-colors ${
                pqcEnabled ? 'bg-green-500/5 border-green-500/20' : 'bg-orange-500/5 border-orange-500/20'
              }`}>
                <div className="flex items-start gap-3">
                  {pqcEnabled ? (
                    <CheckCircle2 className="text-green-500 flex-shrink-0 mt-0.5" size={18} />
                  ) : (
                    <AlertTriangle className="text-orange-500 flex-shrink-0 mt-0.5" size={18} />
                  )}
                  <div>
                    <p className={`text-sm font-bold ${pqcEnabled ? 'text-green-400' : 'text-orange-400'}`}>
                      {pqcEnabled ? 'Fabric Hardened' : 'Standard Encryption'}
                    </p>
                    <p className="text-xs text-slate-500 mt-1 leading-relaxed">
                      {pqcEnabled
                        ? 'Kyber-768/Dilithium key exchange is active across all distributed nodes.'
                        : 'Currently using standard AES-GCM 256-bit encryption. Enable PQC for future-proof security.'}
                    </p>
                  </div>
                </div>
              </div>

              {!pqcEnabled && (
                <button
                  onClick={handlePqc}
                  disabled={loading}
                  className="w-full py-3 bg-purple-600 hover:bg-purple-500 text-white rounded-xl text-sm font-bold transition shadow-lg shadow-purple-500/20"
                >
                  Enable Post-Quantum Defense
                </button>
              )}
            </div>
          </div>

          {/* Audit Log */}
          <div className="bg-slate-900/50 border border-slate-800 rounded-3xl overflow-hidden shadow-2xl">
            <div className="p-6 border-b border-slate-800 flex items-center justify-between">
                <h3 className="font-bold text-slate-100">Security Audit Log</h3>
                <button
                  onClick={async () => { const result = await api.getAuditLog(); if (result.entries) setAuditLog(result.entries); }}
                  className="p-2 hover:bg-slate-800 rounded-lg transition text-slate-500 hover:text-white"
                  aria-label="Refresh audit log"
                >
                  <RefreshCw size={14} aria-hidden="true" />
                </button>
            </div>
            <div className="divide-y divide-slate-800/50 font-mono text-[10px]">
                {auditLog.length === 0 ? (
                    <div className="px-6 py-8 text-center text-slate-600">No audit events yet</div>
                ) : (
                    auditLog.map((log, i) => (
                        <div key={i} className="px-6 py-4 flex items-center justify-between hover:bg-slate-800/20 transition-colors">
                            <div className="flex items-center gap-4">
                                <span className="text-slate-500 w-16">{log.time}</span>
                                <span className="text-blue-400 font-bold">{log.event}</span>
                                <span className="text-slate-600">{log.ip}</span>
                                {log.detail && <span className="text-slate-700">{log.detail}</span>}
                            </div>
                            <span className={`px-2 py-0.5 rounded ${
                                log.status === 'SUCCESS' || log.status === 'AUTHENTICATED'
                                    ? 'bg-green-500/10 text-green-500'
                                    : 'bg-yellow-500/10 text-yellow-500'
                            }`}>{log.status}</span>
                        </div>
                    ))
                )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
