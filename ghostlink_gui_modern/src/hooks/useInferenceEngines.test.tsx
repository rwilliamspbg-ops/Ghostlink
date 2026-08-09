import { renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { useInferenceEngines } from './useInferenceEngines';

describe('useInferenceEngines', () => {
  it('stays empty/unknown rather than fabricating an Ollama default when the engines endpoint is unreachable', async () => {
    const api = {};

    const { result } = renderHook(() => useInferenceEngines(api, 'vllm'));

    await waitFor(() => {
      expect(result.current.engineHealth).toEqual({ reachable: false, model_count: 0 });
    });

    expect(result.current.engines).toEqual([]);
    expect(result.current.currentEngine).toBeNull();
    expect(result.current.selectedEngine).toBeUndefined();
  });

  it('reports the real active engine once the engines endpoint succeeds', async () => {
    const nativeDescriptor = {
      name: 'native',
      label: 'Native',
      status: 'active',
      default_base_url: null,
      capabilities: {
        streaming: true,
        model_listing: false,
        model_load: true,
        model_unload: true,
        structured_outputs: true,
        tool_calls: true,
      },
    };
    const api = {
      getInferenceEngines: vi.fn().mockResolvedValue({
        current: 'native',
        engines: [nativeDescriptor],
      }),
    };

    const { result } = renderHook(() => useInferenceEngines(api));

    await waitFor(() => {
      expect(result.current.currentEngine).toBe('native');
    });

    expect(result.current.selectedEngine).toEqual(nativeDescriptor);
  });

  it('keeps the last known-good engine instead of reverting to Ollama on a later transient failure', async () => {
    const nativeDescriptor = {
      name: 'native',
      label: 'Native',
      status: 'active',
      default_base_url: null,
      capabilities: {
        streaming: true,
        model_listing: false,
        model_load: true,
        model_unload: true,
        structured_outputs: true,
        tool_calls: true,
      },
    };
    const getInferenceEngines = vi
      .fn()
      .mockResolvedValueOnce({ current: 'native', engines: [nativeDescriptor] })
      .mockResolvedValueOnce({ engines: [], current: '', error: 'network error' });
    const api = { getInferenceEngines };

    const { result, rerender } = renderHook(() => useInferenceEngines(api));

    await waitFor(() => {
      expect(result.current.currentEngine).toBe('native');
    });

    await result.current.refreshEngines();
    rerender();

    expect(result.current.currentEngine).toBe('native');
    expect(result.current.selectedEngine).toEqual(nativeDescriptor);
  });
});
