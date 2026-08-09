import { useCallback, useRef, useState } from 'react';
import { ApiError } from '../types/api';

interface RetryOptions {
  maxRetries?: number;
  baseDelay?: number;
  maxDelay?: number;
  retryableStatuses?: number[];
  onRetry?: (attempt: number, error: Error) => void;
  onError?: (error: Error) => void;
}

interface RetryState {
  isRetrying: boolean;
  attempt: number;
  lastError: Error | null;
}

export function useApiRetry<T extends (...args: any[]) => Promise<any>>(
  apiCall: T,
  options: RetryOptions = {}
): [
  (...args: Parameters<T>) => Promise<ReturnType<T>>,
  RetryState
] {
  const {
    maxRetries = 3,
    baseDelay = 1000,
    maxDelay = 30000,
    retryableStatuses = [408, 429, 500, 502, 503, 504],
    onRetry,
    onError,
  } = options;

  const [state, setState] = useState<RetryState>({
    isRetrying: false,
    attempt: 0,
    lastError: null,
  });

  const abortControllerRef = useRef<AbortController | null>(null);
  const isMountedRef = useRef(true);

  const calculateDelay = useCallback((attempt: number): number => {
    const delay = Math.min(baseDelay * Math.pow(2, attempt), maxDelay);
    const jitter = delay * 0.1 * Math.random();
    return Math.floor(delay + jitter);
  }, [baseDelay, maxDelay]);

  const isRetryable = useCallback((error: any): boolean => {
    if (error instanceof ApiError) {
      return error.isRetryable && retryableStatuses.includes(error.status ?? 0);
    }
    if (error?.response?.status) {
      return retryableStatuses.includes(error.response.status);
    }
    if (error?.code) {
      return ['ECONNABORTED', 'ETIMEDOUT', 'ENOTFOUND', 'ENETUNREACH', 'EAI_AGAIN'].includes(error.code);
    }
    return error?.message?.includes('network') || error?.message?.includes('timeout');
  }, [retryableStatuses]);

  const executeWithRetry = useCallback(
    async (...args: Parameters<T>): Promise<ReturnType<T>> => {
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }
      abortControllerRef.current = new AbortController();

      let lastError: Error;
      let attempt = 0;

      while (attempt <= maxRetries) {
        try {
          if (!isMountedRef.current) throw new Error('Component unmounted');
          
          setState({ isRetrying: attempt > 0, attempt, lastError: null });
          
          const result = await apiCall(...args);
          
          if (attempt > 0) {
            setState({ isRetrying: false, attempt: 0, lastError: null });
          }
          return result;
        } catch (error: any) {
          lastError = error instanceof Error ? error : new Error(String(error));
          
          if (attempt === maxRetries || !isRetryable(error)) {
            setState({ isRetrying: false, attempt: 0, lastError });
            onError?.(lastError);
            throw error;
          }

          onRetry?.(attempt + 1, lastError);
          
          const delay = calculateDelay(attempt);
          await new Promise(resolve => setTimeout(resolve, delay));
          
          if (!isMountedRef.current) {
            // eslint-disable-next-line preserve-caught-error -- this isn't re-wrapping `error`, it's a distinct unmount condition
            throw new Error('Component unmounted during retry');
          }
          
          attempt++;
        }
      }

      throw lastError!;
    },
    [apiCall, maxRetries, isRetryable, calculateDelay, onRetry, onError]
  );

  return [executeWithRetry, state] as const;
}