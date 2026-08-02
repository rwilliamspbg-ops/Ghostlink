import { useCallback, useEffect, useMemo, useState } from 'react';
import { GhostlinkAPI } from '../api';
import { createInferenceEngineDescriptors, InferenceEngineDescriptor, InferenceEngineName } from '../types/engines';

type EngineHealth = {
  reachable: boolean;
  model_count: number;
} | null;

type InferenceEngineApi = Pick<GhostlinkAPI, 'getInferenceEngines' | 'getOllamaHealth' | 'getVllmHealth'>;

export function useInferenceEngines(api: Partial<InferenceEngineApi>, activeEngineOverride: string | null = null) {
  const [engines, setEngines] = useState<InferenceEngineDescriptor[]>(createInferenceEngineDescriptors('ollama'));
  const [currentEngine, setCurrentEngine] = useState<InferenceEngineName>('ollama');
  const [engineHealth, setEngineHealth] = useState<EngineHealth>(null);

  const refreshEngines = useCallback(async () => {
    try {
      const result = api.getInferenceEngines ? await api.getInferenceEngines() : null;
      if (!result || !result.error) {
        setEngines(result?.engines || createInferenceEngineDescriptors('ollama'));
        setCurrentEngine((result?.current as InferenceEngineName) || 'ollama');
      }
    } catch (error) {
      console.error('Failed to load inference engines:', error);
      setEngines(createInferenceEngineDescriptors('ollama'));
      setCurrentEngine('ollama');
    }
  }, [api]);

  const selectedEngineName = (activeEngineOverride || currentEngine) as InferenceEngineName;

  const selectedEngine = useMemo(
    () => engines.find((engine) => engine.name === selectedEngineName) || engines[0],
    [engines, selectedEngineName]
  );

  const refreshEngineHealth = useCallback(
    async (engineName: string = selectedEngineName) => {
      try {
        if (engineName === 'vllm') {
          const result = api.getVllmHealth ? await api.getVllmHealth() : { reachable: false, model_count: 0 };
          setEngineHealth(result);
          return result;
        }

        if (engineName === 'ollama') {
          const result = api.getOllamaHealth ? await api.getOllamaHealth() : { reachable: false, model_count: 0 };
          setEngineHealth(result);
          return result;
        }

        setEngineHealth(null);
        return null;
      } catch (error) {
        console.error('Failed to refresh engine health:', error);
        setEngineHealth({ reachable: false, model_count: 0 });
        return { reachable: false, model_count: 0 };
      }
    },
    [api, selectedEngineName]
  );

  useEffect(() => {
    void refreshEngines();
  }, [refreshEngines]);

  useEffect(() => {
    if (activeEngineOverride === null && currentEngine === 'native') {
      setEngineHealth(null);
      return;
    }

    if (selectedEngineName === 'ollama' || selectedEngineName === 'vllm') {
      void refreshEngineHealth(selectedEngineName);
      return;
    }

    setEngineHealth(null);
  }, [activeEngineOverride, currentEngine, refreshEngineHealth, selectedEngineName]);

  return {
    engines,
    currentEngine,
    selectedEngine,
    engineHealth,
    refreshEngines,
    refreshEngineHealth,
  };
}