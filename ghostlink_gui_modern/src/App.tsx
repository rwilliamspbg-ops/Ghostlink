import { useState, useEffect } from 'react';
import {
  MessageSquare,
  Database,
  BarChart3,
  Clock,
  Network,
  Shield,
  Plus,
  Settings,
  ChevronRight,
  Menu,
  Cpu,
  Zap,
  Wifi,
} from 'lucide-react';
import { useAppStore } from './store';
import { GhostlinkAPI } from './api';
import { ChatTab } from './components/ChatTab';
import { ModelsTab } from './components/ModelsTab';
import { MetricsTab } from './components/MetricsTab';
import { SessionsTab } from './components/SessionsTab';
import { WorkersTab } from './components/WorkersTab';
import { SecurityTab } from './components/SecurityTab';
import { SettingsTab } from './components/SettingsTab';
import { ErrorBoundary, OfflineBanner, useOnlineStatus } from './components/ErrorBoundary';

const tabs = [
  { label: 'Chat', icon: MessageSquare, id: 0 },
  { label: 'Models', icon: Database, id: 1 },
  { label: 'Metrics', icon: BarChart3, id: 2 },
  { label: 'Sessions', icon: Clock, id: 3 },
  { label: 'Workers', icon: Network, id: 4 },
  { label: 'Security', icon: Shield, id: 5 },
  { label: 'Settings', icon: Settings, id: 6 },
];

const LOADING_STEPS = [
  'Connecting to backend...',
  'Scanning available models...',
  'Initializing inference engine...',
  'Establishing real-time metrics...',
];

function SplashScreen() {
  const [step, setStep] = useState(0);
  const [dots, setDots] = useState('');

  useEffect(() => {
    const stepInterval = setInterval(() => {
      setStep((s) => (s < LOADING_STEPS.length - 1 ? s + 1 : s));
    }, 1800);
    const dotInterval = setInterval(() => {
      setDots((d) => (d.length >= 3 ? '' : d + '.'));
    }, 400);
    return () => {
      clearInterval(stepInterval);
      clearInterval(dotInterval);
    };
  }, []);

  return (
    <div className="flex items-center justify-center h-screen bg-slate-950">
      <div className="text-center max-w-md">
        {/* Logo */}
        <div className="mb-8 flex justify-center">
          <div className="w-16 h-16 bg-gradient-to-br from-blue-600 to-blue-800 rounded-2xl flex items-center justify-center shadow-lg shadow-blue-600/20">
            <span className="text-3xl font-bold text-white">G</span>
          </div>
        </div>

        {/* Title */}
        <h1 className="text-2xl font-bold text-white mb-2">Ghostlink Studio</h1>
        <p className="text-sm text-slate-500 mb-8">Distributed LLM Inference Fabric</p>

        {/* Loading animation */}
        <div className="relative mb-8 flex justify-center">
          <div className="w-12 h-12 border-[3px] border-slate-800 border-t-blue-500 rounded-full animate-spin" />
        </div>

        {/* Connection status */}
        <div className="space-y-3">
          {LOADING_STEPS.map((s, i) => (
            <div key={i} className="flex items-center gap-3 text-sm">
              <div
                className={`w-5 h-5 rounded-full border-2 flex items-center justify-center shrink-0 transition-all duration-500 ${
                  i < step
                    ? 'bg-green-500/20 border-green-500 text-green-400'
                    : i === step
                    ? 'border-blue-500 text-blue-400'
                    : 'border-slate-700 text-slate-600'
                }`}
              >
                {i < step ? (
                  <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={3}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7" />
                  </svg>
                ) : i === step ? (
                  <div className="w-1.5 h-1.5 bg-blue-400 rounded-full animate-pulse" />
                ) : null}
              </div>
              <span
                className={`transition-colors duration-500 ${
                  i < step ? 'text-slate-400' : i === step ? 'text-slate-300' : 'text-slate-600'
                }`}
              >
                {s}
                {i === step && (
                  <span className="inline-block w-4 text-left text-blue-400">{dots}</span>
                )}
              </span>
            </div>
          ))}
        </div>

        {/* Footer */}
        <div className="mt-10 flex items-center justify-center gap-4 text-xs text-slate-600">
          <span className="flex items-center gap-1"><Cpu size={12} /> GPU</span>
          <span className="flex items-center gap-1"><Zap size={12} /> NPU</span>
          <span className="flex items-center gap-1"><Wifi size={12} /> Cluster</span>
        </div>
      </div>
    </div>
  );
}

function App() {
  const { currentModel, activeTab, setActiveTab, setModels, setApiBase, setMetrics, setWorkers, setSessions, setBackendOnline } = useAppStore();
  const [api, setApi] = useState<GhostlinkAPI | null>(null);
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const isOnline = useOnlineStatus();

  // CRITICAL FIX: Initialize API base on app load
  useEffect(() => {
    const detectedApiBase = import.meta.env.VITE_GHOSTLINK_API_BASE || (window as any)._env_?.GHOSTLINK_API_BASE || 'http://127.0.0.1:8003';
    setApiBase(detectedApiBase);
    setApi(new GhostlinkAPI(detectedApiBase));
  }, [setApiBase]);

  // Fetch models on app load and setup periodic refresh
  useEffect(() => {
    if (!api) return;

    const fetchModels = async () => {
      const result = await api.getModels();
      if (!result.error) {
        setModels(result.models);
      }
    };

    const fetchMetrics = async () => {
      const result = await api.getMetrics();
      if (!result.error) {
        setMetrics(result.metrics);
      }
    };

    const fetchWorkers = async () => {
      const result = await api.getWorkers();
      if (!result.error) {
        setWorkers(result.workers);
      }
    };

    const fetchSessions = async () => {
      const result = await api.getSessions();
      if (!result.error) {
        setSessions(result.sessions);
      }
    };
    
    fetchModels();
    fetchMetrics();
    fetchWorkers();
    fetchSessions();
    
    // Poll for health check every 30s
    const checkHealth = async () => {
      const result = await api.getHealth();
      setBackendOnline(result.success);
    };
    checkHealth();
    const healthInterval = setInterval(checkHealth, 30000);

    // Poll for real-time metrics every 5s
    const metricsInterval = setInterval(() => {
      fetchMetrics();
      fetchSessions();
    }, 5000);

    // Poll for workers every 15s
    const workersInterval = setInterval(() => {
      fetchWorkers();
    }, 15000);

    return () => {
      clearInterval(healthInterval);
      clearInterval(metricsInterval);
      clearInterval(workersInterval);
    };
  }, [api, setModels, setMetrics, setWorkers, setSessions, setBackendOnline]);

  const renderTab = () => {
    if (!api) {
      return <SplashScreen />;
    }

    switch (activeTab) {
      case 0:
        return <ChatTab api={api} />;
      case 1:
        return <ModelsTab api={api} />;
      case 2:
        return <MetricsTab api={api} />;
      case 3:
        return <SessionsTab api={api} />;
      case 4:
        return <WorkersTab api={api} />;
      case 5:
        return <SecurityTab api={api} />;
      case 6:
        return <SettingsTab api={api} />;
      default:
        return null;
    }
  };

  return (
    <ErrorBoundary>
      <div className="flex h-screen bg-slate-950 text-slate-100 font-sans overflow-hidden relative">
        <OfflineBanner isOnline={isOnline} />
        {/* Sidebar */}
      <div
        className={`${
          sidebarOpen ? 'w-64' : 'w-0'
        } transition-all duration-300 bg-slate-900 border-r border-slate-800 flex flex-col overflow-hidden relative`}
      >
        <div className="p-4 flex flex-col h-full">
          <div className="flex items-center gap-3 mb-8 px-2">
            <div className="w-8 h-8 bg-blue-600 rounded-lg flex items-center justify-center font-bold text-white">G</div>
            <h1 className="text-lg font-bold truncate">Ghostlink</h1>
          </div>

          <button
            onClick={() => setActiveTab(0)}
            className="flex items-center justify-between w-full p-3 bg-slate-800 hover:bg-slate-700 rounded-xl transition mb-6 group"
          >
            <div className="flex items-center gap-3">
              <Plus size={18} className="text-slate-400 group-hover:text-white" />
              <span className="text-sm font-medium">New Chat</span>
            </div>
            <div className="text-[10px] text-slate-500 bg-slate-950 px-1.5 py-0.5 rounded border border-slate-700">Ctrl K</div>
          </button>

          <nav className="flex-1 space-y-1">
            {tabs.map((tab) => {
              const Icon = tab.icon;
              return (
                <button
                  key={tab.id}
                  onClick={() => setActiveTab(tab.id)}
                  className={`flex items-center gap-3 w-full px-3 py-2.5 rounded-xl transition text-sm font-medium ${
                    activeTab === tab.id
                      ? 'bg-blue-600/10 text-blue-400'
                      : 'text-slate-400 hover:bg-slate-800 hover:text-slate-200'
                  }`}
                >
                  <Icon size={18} />
                  <span>{tab.label}</span>
                </button>
              );
            })}
          </nav>

          <div className="mt-auto pt-4 border-t border-slate-800 space-y-1">
            <div className="flex items-center gap-3 px-3 py-3 mt-2 rounded-xl bg-slate-800/50">
                <div className="w-8 h-8 bg-gradient-to-br from-orange-400 to-pink-500 rounded-full flex items-center justify-center text-xs font-bold text-white uppercase">
                    {currentModel.substring(0, 1)}
                </div>
                <div className="flex-1 min-w-0 text-left">
                    <p className="text-xs font-bold truncate">Principal Engineer</p>
                    <p className="text-[10px] text-slate-500 truncate">{currentModel}</p>
                </div>
                <ChevronRight size={14} className="text-slate-600" />
            </div>
          </div>
        </div>
      </div>

      {/* Main Area */}
      <div className="flex-1 flex flex-col min-w-0 relative">
        {/* Toggle Sidebar Button (Float) */}
        {!sidebarOpen && (
            <button
                onClick={() => setSidebarOpen(true)}
                className="absolute left-4 top-4 z-10 p-2 bg-slate-900 border border-slate-800 rounded-lg text-slate-400 hover:text-white transition"
            >
                <Menu size={20} />
            </button>
        )}

        {sidebarOpen && (
            <button
                onClick={() => setSidebarOpen(false)}
                className="absolute left-[-12px] top-1/2 -translate_y-1/2 z-20 p-1 bg-slate-800 border border-slate-700 rounded-full text-slate-500 hover:text-white transition"
            >
                <ChevronRight size={16} className="rotate-180" />
            </button>
        )}

        {/* Content */}
        <main className="flex-1 overflow-hidden">
          {renderTab()}
        </main>
      </div>
    </div>
  </ErrorBoundary>
);
}

export default App;
