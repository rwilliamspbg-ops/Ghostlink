export interface RetryConfig {
  maxRetries: number;
  baseDelay: number;
  maxDelay: number;
  retryableStatuses: number[];
  retryableErrors: string[];
}

export interface CircuitBreakerConfig {
  failureThreshold: number;
  successThreshold: number;
  timeout: number;
}

export interface RequestDeduplicationConfig {
  enabled: boolean;
  ttl: number;
}

export interface ApiClientConfig {
  baseURL: string;
  timeout: number;
  retry: RetryConfig;
  circuitBreaker: CircuitBreakerConfig;
  deduplication: RequestDeduplicationConfig;
}

export class ApiError extends Error {
  constructor(
    message: string,
    public readonly status: number | null,
    public readonly code: string,
    public readonly isRetryable: boolean,
    public readonly originalError: Error | null
  ) {
    super(message);
    this.name = 'ApiError';
    Object.setPrototypeOf(this, ApiError.prototype);
  }

  static fromAxiosError(error: any): ApiError {
    const status = error.response?.status ?? null;
    const data = error.response?.data;
    const message = data?.error || error.message || 'Unknown error';
    const code = data?.code || 'UNKNOWN_ERROR';
    
    const retryableStatuses = [408, 429, 500, 502, 503, 504];
    const retryableErrors = ['ECONNABORTED', 'ETIMEDOUT', 'ENOTFOUND', 'ENETUNREACH', 'EAI_AGAIN'];
    const isRetryable = retryableStatuses.includes(status) || retryableErrors.includes(error.code);

    return new ApiError(message, status, code, isRetryable, error);
  }

  static fromFetchError(error: any): ApiError {
    const retryableErrors = ['AbortError', 'TypeError'];
    const isRetryable = retryableErrors.includes(error.name) || error.message?.includes('network');
    
    return new ApiError(
      error.message || 'Network error',
      null,
      error.name || 'NETWORK_ERROR',
      isRetryable,
      error
    );
  }
}

export interface CircuitBreakerState {
  failures: number;
  successes: number;
  state: 'closed' | 'open' | 'half-open';
  lastFailureTime: number;
}

export interface PendingRequest {
  promise: Promise<any>;
  timestamp: number;
}

export const DEFAULT_RETRY_CONFIG: RetryConfig = {
  maxRetries: 3,
  baseDelay: 1000,
  maxDelay: 30000,
  retryableStatuses: [408, 429, 500, 502, 503, 504],
  retryableErrors: ['ECONNABORTED', 'ETIMEDOUT', 'ENOTFOUND', 'ENETUNREACH', 'EAI_AGAIN'],
};

export const DEFAULT_CIRCUIT_BREAKER_CONFIG: CircuitBreakerConfig = {
  failureThreshold: 5,
  successThreshold: 2,
  timeout: 30000,
};

export const DEFAULT_DEDUPLICATION_CONFIG: RequestDeduplicationConfig = {
  enabled: true,
  ttl: 5000,
};

export function validateApiBaseUrl(url: string): { valid: boolean; sanitized: string; error?: string } {
  if (!url || typeof url !== 'string') {
    return { valid: false, sanitized: '', error: 'URL is required' };
  }
  
  const trimmed = url.trim();
  
  if (!trimmed) {
    return { valid: false, sanitized: '', error: 'URL cannot be empty or whitespace' };
  }
  
  try {
    const parsed = new URL(trimmed);
    if (!['http:', 'https:'].includes(parsed.protocol)) {
      return { valid: false, sanitized: '', error: 'URL must use http: or https: protocol' };
    }
    if (!parsed.hostname) {
      return { valid: false, sanitized: '', error: 'URL must have a valid hostname' };
    }
    return { valid: true, sanitized: parsed.toString() };
  } catch {
    return { valid: false, sanitized: '', error: 'Invalid URL format' };
  }
}

export function calculateRetryDelay(attempt: number, config: RetryConfig): number {
  const delay = Math.min(config.baseDelay * Math.pow(2, attempt), config.maxDelay);
  const jitter = delay * 0.1 * Math.random();
  return Math.floor(delay + jitter);
}

export function isRetryableError(error: ApiError, config: RetryConfig): boolean {
  if (!error.isRetryable) return false;
  if (error.status && config.retryableStatuses.includes(error.status)) return true;
  if (error.code && config.retryableErrors.includes(error.code)) return true;
  return false;
}