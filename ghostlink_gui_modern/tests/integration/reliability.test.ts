import { describe, it, expect, vi, beforeEach } from 'vitest';
import { validateApiBaseUrl, calculateRetryDelay, isRetryableError, DEFAULT_RETRY_CONFIG, ApiError } from '../../src/types/api';

describe('Launch Script Validation', () => {
  describe('validateApiBaseUrl', () => {
    it('should accept valid HTTP URLs', () => {
      const result = validateApiBaseUrl('http://127.0.0.1:8003');
      expect(result.valid).toBe(true);
      expect(result.sanitized).toBe('http://127.0.0.1:8003/');
    });

    it('should accept valid HTTPS URLs', () => {
      const result = validateApiBaseUrl('https://api.example.com');
      expect(result.valid).toBe(true);
      expect(result.sanitized).toBe('https://api.example.com/');
    });

    it('should accept URLs with paths', () => {
      const result = validateApiBaseUrl('http://127.0.0.1:8003/api');
      expect(result.valid).toBe(true);
      expect(result.sanitized).toBe('http://127.0.0.1:8003/api');
    });

    it('should reject empty URLs', () => {
      const result = validateApiBaseUrl('');
      expect(result.valid).toBe(false);
      expect(result.error).toContain('required');
    });

    it('should reject whitespace-only URLs', () => {
      const result = validateApiBaseUrl('   ');
      expect(result.valid).toBe(false);
    });

    it('should trim whitespace from URLs', () => {
      const result = validateApiBaseUrl('  http://127.0.0.1:8003  ');
      expect(result.valid).toBe(true);
      expect(result.sanitized).toBe('http://127.0.0.1:8003/');
    });

    it('should reject URLs without protocol', () => {
      const result = validateApiBaseUrl('127.0.0.1:8003');
      expect(result.valid).toBe(false);
      expect(result.error).toBeTruthy();
    });

    it('should reject invalid protocols', () => {
      const result = validateApiBaseUrl('ftp://127.0.0.1:8003');
      expect(result.valid).toBe(false);
      expect(result.error).toBeTruthy();
    });

    it('should reject URLs without hostname', () => {
      const result = validateApiBaseUrl('http://:8003');
      expect(result.valid).toBe(false);
      expect(result.error).toBeTruthy();
    });

    it('should reject URLs with trailing space (the original bug)', () => {
      const result = validateApiBaseUrl('http://127.0.0.1:8003 ');
      expect(result.valid).toBe(true);
      expect(result.sanitized).toBe('http://127.0.0.1:8003/');
    });
  });
});

describe('Retry Delay Calculation', () => {
  it('should calculate exponential backoff with jitter', () => {
    const config = { baseDelay: 1000, maxDelay: 30000, retryableStatuses: [], retryableErrors: [] };
    
    const delay0 = calculateRetryDelay(0, config);
    const delay1 = calculateRetryDelay(1, config);
    const delay2 = calculateRetryDelay(2, config);
    const delay3 = calculateRetryDelay(3, config);

    expect(delay0).toBeGreaterThanOrEqual(900);
    expect(delay0).toBeLessThanOrEqual(1100);
    expect(delay1).toBeGreaterThanOrEqual(1800);
    expect(delay1).toBeLessThanOrEqual(2200);
    expect(delay2).toBeGreaterThanOrEqual(3600);
    expect(delay3).toBeGreaterThanOrEqual(7200);
  });

  it('should cap at maxDelay', () => {
    const config = { baseDelay: 1000, maxDelay: 5000, retryableStatuses: [], retryableErrors: [] };
    const delay = calculateRetryDelay(10, config);
    expect(delay).toBeLessThanOrEqual(5500);
  });
});

describe('isRetryableError', () => {
  it('should return true for retryable status codes', () => {
    const error = new ApiError('Server error', 500, 'INTERNAL_ERROR', true, null);
    expect(isRetryableError(error, DEFAULT_RETRY_CONFIG)).toBe(true);
  });

  it('should return true for retryable error codes', () => {
    const error = new ApiError('Timeout', null, 'ETIMEDOUT', true, null);
    expect(isRetryableError(error, DEFAULT_RETRY_CONFIG)).toBe(true);
  });

  it('should return false for non-retryable status codes', () => {
    const error = new ApiError('Not found', 404, 'NOT_FOUND', false, null);
    expect(isRetryableError(error, DEFAULT_RETRY_CONFIG)).toBe(false);
  });

  it('should return false for non-retryable error codes', () => {
    const error = new ApiError('Auth error', 401, 'UNAUTHORIZED', false, null);
    expect(isRetryableError(error, DEFAULT_RETRY_CONFIG)).toBe(false);
  });
});