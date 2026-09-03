import { useState, useEffect, useRef } from 'react';
import { Plus, ChevronRight, Menu, Cpu, Zap, Wifi, Search } from 'lucide-react';
import { useAppStore } from './store';
import { GhostlinkAPI } from './api';
import { ChatTab } from './components/ChatTab';
import { EditorTab } from './components/EditorTab';
import { ModelsTab } from './components/ModelsTab';
import { MetricsTab } from './components/MetricsTab';
import { SessionsTab } from './components/SessionsTab';
import { WorkersTab } from './components/WorkersTab';
import { SecurityTab } from './components/SecurityTab';
import { SettingsTab } from './components/SettingsTab';
import { McpTab } from './components/McpTab';
import { ErrorBoundary, OfflineBanner, useOnlineStatus } from './components/ErrorBoundary';
import { CommandPalette, NAV_TABS } from './components/CommandPalette';
import { Toaster } from './components/Toaster';
import { resolveApiBase } from './config';

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
  const { currentModel, activeTab, setActiveTab, setModels, setApiBase, setMetrics, setWorkers, setSessions, setBackendOnline, setChatMessages } = useAppStore();
  const [api, setApi] = useState<GhostlinkAPI | null>(null);
  // Starts collapsed on phone-width viewports — at 375px a permanently
  // open 256px sidebar left only ~119px for actual content, which is
  // unusable. Below `md` the sidebar renders as a fixed overlay (see
  // className below) rather than pushing content, so opening it on mobile
  // never squeezes the tab content again.
  const [sidebarOpen, setSidebarOpen] = useState(() =>
    typeof window === 'undefined' ? true : window.innerWidth >= 768
  );
  const closeSidebarOnMobile = () => {
    if (window.innerWidth < 768) setSidebarOpen(false);
  };
  const isOnline = useOnlineStatus();
  const sidebarRef = useRef<HTMLDivElement>(null);
  const sidebarOpenButtonRef = useRef<HTMLButtonElement>(null);

  // On mobile the sidebar renders as a modal-like overlay (see backdrop below),
  // so opening it moves focus in, Escape closes it, and Tab is trapped inside
  // until it closes; on desktop it's a persistent panel and none of this applies.
  useEffect(() => {
    if (!sidebarOpen || window.innerWidth >= 768) return;
    const container = sidebarRef.current;
    if (!container) return;
    const openButton = sidebarOpenButtonRef.current;

    const focusable = () =>
      Array.from(
        container.querySelectorAll<HTMLElement>(
          'a[href], button:not([disabled]), input, select, textarea, [tabindex]:not([tabindex="-1"])'
        )
      );

    focusable()[0]?.focus();

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        setSidebarOpen(false);
        return;
      }
      if (e.key !== 'Tab') return;
      const items = focusable();
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    };

    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('keydown', onKeyDown);
      openButton?.focus();
    };
  }, [sidebarOpen]);

  // CRITICAL FIX: Initialize API base on app load
  useEffect(() => {
    const detectedApiBase = resolveApiBase({
      GHOSTLINK_API_BASE: (window as any)._env_?.GHOSTLINK_API_BASE,
      GHOSTLINK_BACKEND_URL: (window as any)._env_?.GHOSTLINK_BACKEND_URL,
      VITE_GHOSTLINK_API_BASE: import.meta.env.VITE_GHOSTLINK_API_BASE,
      VITE_GHOSTLINK_BACKEND_URL: import.meta.env.VITE_GHOSTLINK_BACKEND_URL,
    });
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
        if (result.current_model && result.current_model !== 'none' && useAppStore.getState().currentModel === 'none') {
          useAppStore.getState().setCurrentModel(result.current_model);
        }
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

    const handleSwitchTab = (e: CustomEvent) => {
      if (typeof e.detail === 'number') {
        setActiveTab(e.detail);
      }
    };
    window.addEventListener('switch-tab', handleSwitchTab as EventListener);


    // Poll metrics every 3s for responsive System Performance gauges
    const metricsInterval = setInterval(() => {
      fetchMetrics();
      fetchSessions();
    }, 3000);

    // Poll for workers every 15s
    const workersInterval = setInterval(() => {
      fetchWorkers();
    }, 15000);

    return () => {
      clearInterval(healthInterval);
      window.removeEventListener('switch-tab', handleSwitchTab as EventListener);
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
      case 7:
        return <McpTab api={api} />;
      case 8:
        return <EditorTab api={api} />;
      default:
        return null;
    }
  };

  return (
    <ErrorBoundary>
      <div className="flex h-screen bg-slate-950 text-slate-100 font-sans overflow-hidden relative">
        <a
          href="#main-content"
          className="sr-only focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-50 focus:px-4 focus:py-2 focus:rounded-lg focus:bg-blue-600 focus:text-white focus:text-sm focus:font-medium"
        >
          Skip to main content
        </a>
        <OfflineBanner isOnline={isOnline} />
        <CommandPalette />
        <Toaster />
        {/* Backdrop — mobile-only, closes the sidebar overlay on tap outside it */}
        {sidebarOpen && (
          <div
            className="fixed inset-0 bg-black/50 z-30 md:hidden"
            onClick={() => setSidebarOpen(false)}
            aria-hidden="true"
          />
        )}
        {/* Sidebar */}
      <div
        id="primary-sidebar"
        ref={sidebarRef}
        className={`${
          sidebarOpen ? 'w-64 max-md:fixed max-md:inset-y-0 max-md:left-0 max-md:z-40 max-md:shadow-2xl' : 'w-0'
        } transition-all duration-300 bg-slate-900 border-r border-slate-800 flex flex-col overflow-hidden relative`}
      >
        <div className="p-4 flex flex-col h-full">
          <div className="flex items-center gap-3 mb-8 px-2">
            <div className="w-8 h-8 bg-blue-600 rounded-lg flex items-center justify-center font-bold text-white">G</div>
            <h1 className="text-lg font-bold truncate">Ghostlink</h1>
          </div>

          <button
            onClick={() => {
              const messages = useAppStore.getState().chatMessages;
              if (messages.length === 0 || window.confirm('Are you sure you want to clear the current chat? This will permanently delete your active conversation.')) {
                setChatMessages([]);
                setActiveTab(0);
                closeSidebarOnMobile();
              }
            }}
            title="Start a new chat session (Ctrl+Shift+O)"
            aria-label="Start a new chat"
            className="flex items-center justify-between w-full p-3 bg-slate-800 hover:bg-slate-700 rounded-xl transition mb-2 group focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
          >
            <div className="flex items-center gap-3">
              <Plus size={18} className="text-slate-400 group-hover:text-white" aria-hidden="true" />
              <span className="text-sm font-medium">New Chat</span>
            </div>
            <div className="text-[10px] text-slate-500 bg-slate-950 px-1.5 py-0.5 rounded border border-slate-700" title="Shortcut for New Chat">Ctrl⇧O</div>
          </button>

          <button
            onClick={() => {
              window.dispatchEvent(new CustomEvent('open-command-palette'));
            }}
            title="Search commands (Ctrl+K)"
            aria-label="Search and run commands"
            className="flex items-center justify-between w-full p-3 bg-slate-800/40 hover:bg-slate-800 border border-slate-800/60 rounded-xl transition mb-6 group focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
          >
            <div className="flex items-center gap-3">
              <Search size={18} className="text-slate-500 group-hover:text-slate-300" aria-hidden="true" />
              <span className="text-sm font-medium text-slate-400 group-hover:text-slate-200">Search commands...</span>
            </div>
            <div className="text-[10px] text-slate-500 bg-slate-950 px-1.5 py-0.5 rounded border border-slate-700" title="Shortcut for Command Palette">Ctrl K</div>
          </button>

          <nav className="flex-1 space-y-1" aria-label="Primary">
            {NAV_TABS.map((tab) => {
              const Icon = tab.icon;
              const isActive = activeTab === tab.id;
              return (
                <button
                  key={tab.id}
                  onClick={() => { setActiveTab(tab.id); closeSidebarOnMobile(); }}
                  aria-current={isActive ? 'page' : undefined}
                  aria-label={tab.label}
                  title={tab.label}
                  className={`flex items-center gap-3 w-full px-3 py-2.5 rounded-xl transition text-sm font-medium focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none ${
                    isActive
                      ? 'bg-blue-600/10 text-blue-400'
                      : 'text-slate-400 hover:bg-slate-800 hover:text-slate-200'
                  }`}
                >
                  <Icon size={18} aria-hidden="true" />
                  <span>{tab.label}</span>
                </button>
              );
            })}
          </nav>

          <div className="mt-auto pt-4 border-t border-slate-800 space-y-1">
            <button
                onClick={() => { setActiveTab(6); closeSidebarOnMobile(); }}
                title="Open Settings"
                aria-label={`Active model: ${currentModel}. Open Settings.`}
                className="flex items-center gap-3 w-full px-3 py-3 mt-2 rounded-xl bg-slate-800/50 hover:bg-slate-800 transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
            >
                <div className="w-8 h-8 bg-gradient-to-br from-orange-400 to-pink-500 rounded-full flex items-center justify-center text-xs font-bold text-white uppercase shrink-0">
                    {currentModel.substring(0, 1)}
                </div>
                <div className="flex-1 min-w-0 text-left">
                    <p className="text-xs font-bold truncate">Active Model</p>
                    <p className="text-[10px] text-slate-500 truncate">{currentModel}</p>
                </div>
                <ChevronRight size={14} className="text-slate-600" aria-hidden="true" />
            </button>
          </div>
        </div>
      </div>

      {/* Main Area */}
      <div className="flex-1 flex flex-col min-w-0 relative">
        {/* Toggle Sidebar Button (Float) */}
        {!sidebarOpen && (
            <button
                ref={sidebarOpenButtonRef}
                onClick={() => setSidebarOpen(true)}
                title="Open sidebar"
                aria-label="Open sidebar"
                aria-expanded={false}
                aria-controls="primary-sidebar"
                className="absolute left-4 top-4 z-10 p-2 bg-slate-900 border border-slate-800 rounded-lg text-slate-400 hover:text-white transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
            >
                <Menu size={20} aria-hidden="true" />
            </button>
        )}

        {sidebarOpen && (
            <button
                onClick={() => setSidebarOpen(false)}
                title="Collapse sidebar"
                aria-label="Collapse sidebar"
                aria-expanded={true}
                aria-controls="primary-sidebar"
                className="absolute left-[-12px] top-1/2 -translate-y-1/2 z-20 p-1 bg-slate-800 border border-slate-700 rounded-full text-slate-500 hover:text-white transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
            >
                <ChevronRight size={16} className="rotate-180" aria-hidden="true" />
            </button>
        )}

        {/* Content */}
        <main id="main-content" tabIndex={-1} className="flex-1 overflow-hidden">
          {renderTab()}
        </main>
      </div>
    </div>
  </ErrorBoundary>
);
}

export default App;
