import React, { useState, useEffect } from 'react';
import { AlertCircle, Wifi, WifiOff } from 'lucide-react';
import { useAppStore } from '../store';

interface HealthIndicatorProps {
  api: any;
}

export const HealthIndicator: React.FC<HealthIndicatorProps> = ({ api }) => {
  const { setBackendOnline, setCurrentModel, setUptime, backendOnline } = useAppStore();
  const [lastPing, setLastPing] = useState('never');

  useEffect(() => {
    const checkHealth = async () => {
      const result = await api.getHealth();
      if (result.success) {
        setBackendOnline(true);
        setCurrentModel(result.data.current_model || 'none');
        setUptime(result.data.uptime_s || 0);
        setLastPing(new Date().toLocaleTimeString());
      } else {
        setBackendOnline(false);
        setLastPing('offline');
      }
    };

    checkHealth();
    const interval = setInterval(checkHealth, 3000);
    return () => clearInterval(interval);
  }, [api, setBackendOnline, setCurrentModel, setUptime]);

  return (
    <div className="flex items-center gap-3 p-4 bg-slate-900 rounded-lg border border-slate-700">
      {backendOnline ? (
        <div className="flex items-center gap-2 text-emerald-400">
          <Wifi size={20} />
          <span>● Online</span>
        </div>
      ) : (
        <div className="flex items-center gap-2 text-red-400">
          <WifiOff size={20} />
          <span>● Offline</span>
        </div>
      )}
      <span className="text-sm text-slate-400">Last ping: {lastPing}</span>
    </div>
  );
};

interface StatusMessageProps {
  message: string;
  type?: 'success' | 'error' | 'info';
}

export const StatusMessage: React.FC<StatusMessageProps> = ({ message, type = 'info' }) => {
  const bgColor = {
    success: 'bg-emerald-900',
    error: 'bg-red-900',
    info: 'bg-blue-900',
  }[type];

  const textColor = {
    success: 'text-emerald-200',
    error: 'text-red-200',
    info: 'text-blue-200',
  }[type];

  const icon = type === 'error' ? <AlertCircle size={16} /> : null;

  return (
    <div className={`${bgColor} ${textColor} p-3 rounded border border-opacity-30 flex items-center gap-2`}>
      {icon}
      <span>{message}</span>
    </div>
  );
};
