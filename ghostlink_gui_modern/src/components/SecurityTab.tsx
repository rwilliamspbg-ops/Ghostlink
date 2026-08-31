import { HealthPanel } from './HealthPanel';
import React, { useState, useEffect } from 'react';
import { Shield, Key, RefreshCw, AlertTriangle, Eye, EyeOff, CheckCircle2, Copy, Check } from 'lucide-react';
import { useAppStore } from '../store';

interface AuditEntry {
  event: string;
  status: string;
  ip: string;
  time: string;
  detail?: string;
}

export const SecurityTab: React.FC<{ api: any }> = ({ api }) => {
  const { addToast } = useAppStore();
  const [loading, setLoading] = useState(false);
  const [showToken, setShowShowToken] = useState(false);
  const [pqcEnabled, setPqcEnabled] = useState(false);
  // null (not an empty/placeholder-looking string) until a real token is
  // fetched — this used to default to a fake JWT-shaped literal that
  // rendered as-is the moment "Show token" was clicked, before Refresh was
  // ever called, which reads as a real credential when it's actually inert.
  const [token, setToken] = useState<string | null>(null);
  const [auditLog, setAuditLog] = useState<AuditEntry[]>([]);
  const [apiKeyInput, setApiKeyInput] = useState('');
  const [apiKeySaved, setApiKeySaved] = useState(false);
  const [showApiKey, setShowApiKey] = useState(false);
  const [pqcRestartRequired, setPqcRestartRequired] = useState(false);
  const [tokenCopied, setTokenCopied] = useState(false);

  const handleCopyToken = async () => {
    if (!token) return;
    try {
      await navigator.clipboard.writeText(token);
      setTokenCopied(true);
      setTimeout(() => setTokenCopied(false), 2000);
    } catch {
      addToast({ type: 'error', message: 'Failed to copy access token to clipboard' });
    }
  };

  useEffect(() => {
    setApiKeyInput(api.getApiKey?.() ?? '');
  }, [api]);

  const handleSaveApiKey = () => {
    api.setApiKey?.(apiKeyInput.trim());
    setApiKeySaved(true);
    setTimeout(() => setApiKeySaved(false), 2000);
  };

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
      if (result.success) {
        setToken(result.data?.token || token);
      } else {
        addToast({ type: 'error', message: result.error || 'Failed to refresh token' });
      }
    } finally {
      setLoading(false);
    }
  };

  const handlePqc = async () => {
    setLoading(true);
    try {
      const result = await api.enablePQC();
      // Saves the enable_tls setting for next restart — the listener is
      // bound once at server startup, so this isn't live yet. Reflect
      // that honestly rather than flipping straight to "enabled".
      if (result.success && result.data?.restart_required) {
        setPqcRestartRequired(true);
      } else if (result.success) {
        setPqcEnabled(true);
      } else {
        addToast({ type: 'error', message: result.error || 'Failed to enable PQC' });
      }
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex flex-col h-full bg-slate-950">
      <div className="flex items-center justify-between px-6 py-4 border-b border-slate-900 sticky top-0 bg-slate-950/50 backdrop-blur-md z-10">
        <h2 className="text-xl font-bold text-white">Security & Access</h2>
      </div>

      <div className="flex-1 overflow-y-auto p-6" tabIndex={0} role="region" aria-label="Security">
        <div className="max-w-5xl mx-auto space-y-6">
          <HealthPanel api={api} onNavigateToTab={(tabName) => {
            if (tabName === "models") window.dispatchEvent(new CustomEvent("switch-tab", { detail: 1 }));
            if (tabName === "settings") window.dispatchEvent(new CustomEvent("switch-tab", { detail: 6 }));
          }} />
          {/* API Key Section */}
          <div className="bg-slate-900/50 border border-slate-800 rounded-3xl p-6 space-y-4">
            <div className="flex items-center gap-3">
              <div className="p-3 bg-blue-500/10 rounded-2xl text-blue-400">
                <Key size={24} />
              </div>
              <div>
                <h3 className="font-bold text-slate-100">API Key</h3>
                <p className="text-xs text-slate-500">
                  Required for every request to this server. Printed once in the server's console output
                  on first run (or check <code className="text-slate-400 bg-slate-800 px-1 rounded font-mono">api_key.txt</code> next
                  to the running server) — paste it here to authenticate this browser.
                </p>
              </div>
            </div>
            <div className="flex gap-2">
              <input
                type={showApiKey ? 'text' : 'password'}
                value={apiKeyInput}
                onChange={(e) => setApiKeyInput(e.target.value)}
                placeholder="Paste the API key printed at server startup"
                aria-label="API key"
                className="flex-1 bg-slate-950 border border-slate-800 rounded-xl px-4 py-2.5 text-sm font-mono text-slate-200 placeholder:text-slate-600 focus:outline-none focus:border-blue-500 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
              />
              <button
                onClick={() => setShowApiKey(!showApiKey)}
                className="px-3 text-slate-500 hover:text-white transition rounded-xl focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                aria-label={showApiKey ? 'Hide API key' : 'Show API key'}
                aria-pressed={showApiKey}
              >
                {showApiKey ? <EyeOff size={16} aria-hidden="true" /> : <Eye size={16} aria-hidden="true" />}
              </button>
              <button
                onClick={handleSaveApiKey}
                className="px-5 py-2.5 bg-blue-600 hover:bg-blue-500 text-white rounded-xl text-sm font-bold transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
              >
                {apiKeySaved ? 'Saved' : 'Save'}
              </button>
            </div>
          </div>

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
                  <div className="flex items-center gap-1.5">
                    {token && (
                      <button
                        onClick={handleCopyToken}
                        className={`p-1 transition rounded-lg focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none ${
                          tokenCopied ? 'text-green-400 hover:text-green-300' : 'text-slate-500 hover:text-white'
                        }`}
                        aria-label={tokenCopied ? 'Copied token' : 'Copy access token'}
                        title={tokenCopied ? 'Copied!' : 'Copy access token'}
                      >
                        {tokenCopied ? <Check size={14} aria-hidden="true" /> : <Copy size={14} aria-hidden="true" />}
                      </button>
                    )}
                    <button
                      onClick={() => setShowShowToken(!showToken)}
                      className="p-1 text-slate-500 hover:text-white transition rounded-lg focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                      aria-label={showToken ? 'Hide token' : 'Show token'}
                      aria-pressed={showToken}
                      title={showToken ? 'Hide token' : 'Show token'}
                    >
                      {showToken ? <EyeOff size={14} aria-hidden="true" /> : <Eye size={14} aria-hidden="true" />}
                    </button>
                  </div>
                </div>
                <div className="font-mono text-xs break-all text-slate-400 pr-8">
                  {showToken
                    ? token ?? 'Not yet fetched — click Refresh below'
                    : '••••••••••••••••••••••••••••••••••••••••••••••••'}
                </div>
              </div>

              <button
                onClick={handleRefresh}
                disabled={loading}
                className="w-full flex items-center justify-center gap-2 py-3 bg-slate-800 hover:bg-slate-700 text-white rounded-xl text-sm font-bold transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
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
                  <p className="text-xs text-slate-500">HTTPS with a real PQC-hybrid key exchange (X25519MLKEM768)</p>
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
                      {pqcEnabled ? 'HTTPS + PQC-hybrid active' : 'Plain HTTP — no transport encryption'}
                    </p>
                    <p className="text-xs text-slate-500 mt-1 leading-relaxed">
                      {pqcEnabled
                        ? 'This server is running HTTPS with rustls configured to prefer the X25519MLKEM768 post-quantum hybrid key-exchange group (the same mechanism Chrome/Cloudflare/AWS use). Bearer-token auth on every request is independent of this and always active regardless.'
                        : pqcRestartRequired
                          ? 'Saved — restart the server for the HTTPS/PQC-hybrid listener to take effect. Bearer-token auth on every request is already active regardless.'
                          : 'This server is on plain HTTP — requests aren’t encrypted in transit (bearer-token auth still applies, but a token sent over plain HTTP can be observed). Enabling generates a local self-signed certificate.'}
                    </p>
                  </div>
                </div>
              </div>

              {!pqcEnabled && !pqcRestartRequired && (
                <button
                  onClick={handlePqc}
                  disabled={loading}
                  className="w-full py-3 bg-purple-600 hover:bg-purple-500 text-white rounded-xl text-sm font-bold transition shadow-lg shadow-purple-500/20 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                >
                  Enable HTTPS + PQC-Hybrid TLS
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
                  className="p-2 hover:bg-slate-800 rounded-lg transition text-slate-500 hover:text-white focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                  aria-label="Refresh audit log"
                  title="Refresh audit log"
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
