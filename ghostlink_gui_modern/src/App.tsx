import React, { useState, useEffect } from 'react';
import {
  MessageSquare,
  Database,
  BarChart3,
  Clock,
  Network,
  Shield,
  Terminal,
} from 'lucide-react';
import { useAppStore } from './store';
import { GhostlinkAPI } from './api';
import { HealthIndicator } from './components/StatusIndicator';
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
  const { apiBase, currentModel, uptime, activeTab, setActiveTab, setModels } = useAppStore();
  const [api] = useState(() => new GhostlinkAPI(apiBase));

  // Fetch models on app load
  useEffect(() => {
    const fetchModels = async () => {
      const result = await api.getModels();
      if (!result.error) {
        setModels(result.models);
      }
    };
    fetchModels();
  }, [api, setModels]);

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
    <div className="min-h-screen bg-slate-950">
      {/* Header */}
      <div className="bg-gradient-to-r from-slate-900 via-slate-800 to-slate-900 border-b border-slate-700 px-6 py-4">
        <div className="max-w-7xl mx-auto">
          <div className="flex items-center justify-between mb-4">
            <div className="flex items-center gap-3">
              <Terminal className="text-blue-400" size={32} />
              <div>
                <h1 className="text-2xl font-bold text-white">Ghostlink Studio</h1>
                <p className="text-sm text-slate-400">Advanced Model Management Interface</p>
              </div>
            </div>
            <div className="text-right">
              <p className="text-sm text-slate-300">
                Model: <span className="font-semibold text-blue-400">{currentModel}</span>
              </p>
              <p className="text-sm text-slate-400">Uptime: {uptime}s</p>
            </div>
          </div>
          <HealthIndicator api={api} />
        </div>
      </div>

      {/* Main Content */}
      <div className="max-w-7xl mx-auto p-6">
        {/* Tab Navigation */}
        <div className="flex gap-2 mb-6 overflow-x-auto pb-2">
          {tabs.map((tab) => {
            const Icon = tab.icon;
            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`flex items-center gap-2 px-4 py-2 rounded transition whitespace-nowrap ${
                  activeTab === tab.id
                    ? 'bg-blue-600 text-white'
                    : 'bg-slate-800 text-slate-400 hover:bg-slate-700'
                }`}
              >
                <Icon size={18} />
                {tab.label}
              </button>
            );
          })}
        </div>

        {/* Tab Content */}
        <div className="bg-slate-900 rounded-lg border border-slate-700 p-6 min-h-screen">
          {renderTab()}
        </div>
      </div>
    </div>
  );
}

export default App;
