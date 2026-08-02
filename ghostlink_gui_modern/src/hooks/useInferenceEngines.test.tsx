import { renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { useInferenceEngines } from './useInferenceEngines';

describe('useInferenceEngines', () => {
  it('falls back to Ollama descriptors and unreachable health when inference methods are missing', async () => {
    const api = {};

    const { result } = renderHook(() => useInferenceEngines(api, 'vllm'));

    await waitFor(() => {
      expect(result.current.engines[0].name).toBe('ollama');
    });

    expect(result.current.currentEngine).toBe('ollama');
    expect(result.current.selectedEngine.name).toBe('vllm');
    expect(result.current.engineHealth).toEqual({ reachable: false, model_count: 0 });
  });
});
