import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import './index.css';
import { useAppStore } from './store';

// Check for environment variable for API base
const envApiBase = (window as any)._env_?.GHOSTLINK_API_BASE || import.meta.env.VITE_GHOSTLINK_API_BASE;
if (envApiBase) {
  useAppStore.getState().setApiBase(envApiBase);
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
