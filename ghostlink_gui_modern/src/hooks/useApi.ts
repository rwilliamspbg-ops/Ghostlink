import { useCallback } from 'react';
import { GhostlinkAPI } from '../api';
import { Settings, Model, Metric, Session, Worker } from '../store';

export function useApi(api: GhostlinkAPI) {
  const executeWithRetry = useCallback(
    async <T,>(method: string, ...args: any[]): Promise<T> => {
      return (api as any)[method](...args);
    },
    [api]
  );

  return {
    getHealth: () => executeWithRetry<{ success: boolean; data?: any; error?: string }>('getHealth'),
    getModels: () => executeWithRetry<{ models: Model[]; current_model: string; error?: string }>('getModels'),
    loadModel: (modelName: string) => executeWithRetry<{ success: boolean; data?: any; error?: string }>('loadModel', modelName),
    downloadModel: (modelId: string) => executeWithRetry<{ success: boolean; data?: any; error?: string }>('downloadModel', modelId),
    deleteModel: (modelName: string) => executeWithRetry<{ success: boolean; data?: any; error?: string }>('deleteModel', modelName),
    unloadModel: (modelName: string) => executeWithRetry<{ success: boolean; data?: any; error?: string }>('unloadModel', modelName),
    searchHuggingFace: (query: string) => executeWithRetry<{ models: any[] }>('searchHuggingFace', query),
    discoverPeers: () => executeWithRetry<{ success: boolean; count: number; error?: string }>('discoverPeers'),
    disconnectWorker: (workerId: string) => executeWithRetry<{ success: boolean; data?: any; error?: string }>('disconnectWorker', workerId),
    sendMessage: (payload: any, onToken?: (token: string) => void) => 
      executeWithRetry<{ success: boolean; data?: any; error?: string }>('sendMessage', payload, onToken),
    getMetrics: () => executeWithRetry<{ metrics: Metric; error?: string }>('getMetrics'),
    getSessions: () => executeWithRetry<{ sessions: Session[]; error?: string }>('getSessions'),
    cancelSession: (sessionId: string) => executeWithRetry<{ success: boolean; data?: any; error?: string }>('cancelSession', sessionId),
    getRuntimes: () => executeWithRetry<{ available_runtimes: any[]; error?: string }>('getRuntimes'),
    getSettings: () => executeWithRetry<{ settings: Settings; error?: string }>('getSettings'),
    updateSettings: (settings: Partial<Settings>) => executeWithRetry<{ success: boolean; settings?: Settings; error?: string }>('updateSettings', settings),
    resetSettings: () => executeWithRetry<{ success: boolean; settings?: Settings; error?: string }>('resetSettings'),
    getWorkers: () => executeWithRetry<{ workers: Worker[]; error?: string }>('getWorkers'),
    getDownloadProgress: (modelId: string) => executeWithRetry<{ progress: number; status: string; error?: string }>('getDownloadProgress', modelId),
    addWorker: (host: string, port: number) => executeWithRetry<{ success: boolean; error?: string }>('addWorker', host, port),
    refreshJWT: () => executeWithRetry<{ success: boolean; data?: { token: string }; error?: string }>('refreshJWT'),
    getPQCState: () => executeWithRetry<{ enabled: boolean; error?: string }>('getPQCState'),
    enablePQC: () => executeWithRetry<{ success: boolean; data?: any; error?: string }>('enablePQC'),
    selectRuntime: (runtime: string) => executeWithRetry<{ success: boolean; error?: string; data?: any }>('selectRuntime', runtime),
    getAuditLog: () => executeWithRetry<{ entries: any[]; error?: string }>('getAuditLog'),
  };
}