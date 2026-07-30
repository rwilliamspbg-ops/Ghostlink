import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import './index.css';
import './monacoSetup';
import { useAppStore } from './store';
import { resolveApiBase } from './config';

// Check for environment variable for API base
const envApiBase = resolveApiBase({
  GHOSTLINK_API_BASE: (window as any)._env_?.GHOSTLINK_API_BASE,
  GHOSTLINK_BACKEND_URL: (window as any)._env_?.GHOSTLINK_BACKEND_URL,
  VITE_GHOSTLINK_API_BASE: import.meta.env.VITE_GHOSTLINK_API_BASE,
  VITE_GHOSTLINK_BACKEND_URL: import.meta.env.VITE_GHOSTLINK_BACKEND_URL,
});

useAppStore.getState().setApiBase(envApiBase);

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
