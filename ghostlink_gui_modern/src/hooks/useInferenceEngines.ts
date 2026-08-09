import { useCallback, useEffect, useMemo, useState } from 'react';
import { GhostlinkAPI } from '../api';
import { InferenceEngineDescriptor, InferenceEngineName } from '../types/engines';

type EngineHealth = {
  reachable: boolean;
  model_count: number;
} | null;

type InferenceEngineApi = Pick<GhostlinkAPI, 'getInferenceEngines' | 'getOllamaHealth' | 'getVllmHealth'>;

export function useInferenceEngines(api: Partial<InferenceEngineApi>, activeEngineOverride: string | null = null) {
  // Deliberately start empty/unknown rather than guessing a specific engine
  // (this used to default to a fabricated "Ollama, active" descriptor list,
  // including capabilities that don't even match what the real backend
  // reports for other engines — e.g. its hardcoded native.tool_calls was
  // `false` while the live server reports `true`). Any transient failure
  // to reach /api/inference/engines — a backend restart, one dropped
  // request — then permanently looked identical to "you've configured
  // Ollama, which doesn't support tool calls," with no indication it was
  // actually an error state. Every consumer (ChatTab, SettingsTab,
  // ModelsTab) already null-guards `selectedEngine` and empty-guards
  // `engines`, so leaving both genuinely empty until a real fetch succeeds
  // is safe and honest.
  const [engines, setEngines] = useState<InferenceEngineDescriptor[]>([]);
  const [currentEngine, setCurrentEngine] = useState<InferenceEngineName | null>(null);
  const [engineHealth, setEngineHealth] = useState<EngineHealth>(null);

  const refreshEngines = useCallback(async () => {
    try {
      const result = api.getInferenceEngines ? await api.getInferenceEngines() : null;
      if (result && !result.error && result.engines?.length && result.current) {
        setEngines(result.engines);
        setCurrentEngine(result.current as InferenceEngineName);
      }
      // A failed/empty/errored result leaves prior state untouched —
      // preserving the last known-good engine on a transient failure
      // (or staying empty/unknown before the first successful load)
      // instead of snapping to a fabricated default.
    } catch (error) {
      console.error('Failed to load inference engines:', error);
    }
  }, [api]);

  const selectedEngineName = (activeEngineOverride || currentEngine) as InferenceEngineName | null;

  const selectedEngine = useMemo(
    () => engines.find((engine) => engine.name === selectedEngineName) || engines[0],
    [engines, selectedEngineName]
  );

  const refreshEngineHealth = useCallback(
    async (engineName: string | null = selectedEngineName) => {
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