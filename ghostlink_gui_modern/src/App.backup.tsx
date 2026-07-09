import React, { useState, useEffect } from 'react';
import {
  MessageSquare,
  Database,
  BarChart3,
  Clock,
  Network,
  Shield,
  Plus,
  Search,
  Settings,
  User,
  ChevronRight,
  Menu,
} from 'lucide-react';
import { useAppStore } from './store';
import { GhostlinkAPI } from './api';
import { ChatTab } from './components/ChatTab';
import { ModelsTab } from './components/ModelsTab';
import { MetricsTab } from './components/MetricsTab';
import { SessionsTab } from './components/SessionsTab';
import { WorkersTab } from './components/WorkersTab';
import { SecurityTab } from './components/SecurityTab';

const tabs = [
  { label: 'Chat', icon: MessageSquare, id: 0 },
  { label: 'Models', icon: Database, id: 1 },
  { label: 'Metrics', icon: BarChart3, id: 2 },
  { label: 'Sessions', icon: Clock, id: 3 },
  { label: 'Workers', icon: Network, id: 4 },
  { label: 'Security', icon: Shield, id: 5 },
];

function App() {
  const { apiBase, currentModel, uptime, activeTab, setActiveTab, setModels, setUptime } = useAppStore();
  const [api] = useState(() => new GhostlinkAPI(apiBase));
  const [sidebarOpen, setSidebarOpen] = useState(true);

  // Fetch models on app load
  useEffect(() => {
    const fetchModels = async () => {
      const result = await api.getModels();
      if (!result.error) {
        setModels(result.models);
      }
    };
    fetchModels();

    const interval = setInterval(() => {
        setUptime(uptime + 1);
    }, 1000);
    return () => clearInterval(interval);
  }, [api, setModels, setUptime, uptime]);

  const renderTab = () => {
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
      default:
        return null;
    }
  };

  return (
    <div className="flex h-screen bg-slate-950 text-slate-100 font-sans overflow-hidden">
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

          <button className="flex items-center justify-between w-full p-3 bg-slate-800 hover:bg-slate-700 rounded-xl transition mb-6 group">
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
            <button className="flex items-center gap-3 w-full px-3 py-2.5 rounded-xl text-slate-400 hover:bg-slate-800 hover:text-slate-200 transition text-sm font-medium">
              <Settings size={18} />
              <span>Settings</span>
            </button>
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
                className="absolute left-[-12px] top-1/2 -translate-y-1/2 z-20 p-1 bg-slate-800 border border-slate-700 rounded-full text-slate-500 hover:text-white transition"
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
  );
}

export default App;
