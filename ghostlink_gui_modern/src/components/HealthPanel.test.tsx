import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { HealthPanel } from './HealthPanel';

const { mockAddToast } = vi.hoisted(() => ({
  mockAddToast: vi.fn(),
}));

vi.mock('../store', () => ({
  useAppStore: () => ({
    addToast: mockAddToast,
  }),
}));

describe('HealthPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders system health probes grid when backends are healthy', async () => {
    const mockApi = {
      getHealth: vi.fn().mockResolvedValue({ success: true }),
      getModels: vi.fn().mockResolvedValue({ models: [], current_model: 'llama-3.2-3b' }),
      getInferenceEngines: vi.fn().mockResolvedValue({ engines: [{ name: 'vllm' }] }),
      setApiKey: vi.fn(),
    };

    render(<HealthPanel api={mockApi as any} />);

    expect(await screen.findByText('Reachable')).toBeInTheDocument();
    expect(screen.getByText('Active')).toBeInTheDocument();
    expect(screen.getByText('llama-3.2-3b')).toBeInTheDocument();
  });

  it('displays HTTP 401 recovery form when endpoints return unauthorized', async () => {
    const mockApi = {
      getHealth: vi.fn().mockResolvedValue({ error: '401 Unauthorized' }),
      getModels: vi.fn().mockResolvedValue({ error: '401 Unauthorized' }),
      getInferenceEngines: vi.fn().mockResolvedValue({ error: '401 Unauthorized' }),
      setApiKey: vi.fn(),
    };

    render(<HealthPanel api={mockApi as any} />);

    expect(await screen.findByText(/Authentication Recovery \(HTTP 401\)/i)).toBeInTheDocument();

    const apiKeyInput = screen.getByLabelText(/Recovery API key input/i);
    const submitBtn = screen.getByRole('button', { name: /Apply recovery API key/i });

    expect(submitBtn).toBeDisabled();

    fireEvent.change(apiKeyInput, { target: { value: 'test-admin-key' } });
    expect(submitBtn).not.toBeDisabled();

    await act(async () => {
      fireEvent.click(submitBtn);
    });

    expect(mockApi.setApiKey).toHaveBeenCalledWith('test-admin-key');
    expect(mockAddToast).toHaveBeenCalledWith({
      type: 'success',
      message: 'API key updated. Re-testing health...',
    });
  });

  it('sets aria-busy and displays loader animation on submit button during active probing', async () => {
    let resolveHealth: any;
    const healthPromise = new Promise((resolve) => {
      resolveHealth = resolve;
    });

    const mockApi = {
      getHealth: vi.fn().mockImplementation(() => healthPromise),
      getModels: vi.fn().mockResolvedValue({ models: [] }),
      getInferenceEngines: vi.fn().mockResolvedValue({ engines: [] }),
      setApiKey: vi.fn(),
    };

    render(<HealthPanel api={mockApi as any} />);

    const reprobeBtn = screen.getByRole('button', { name: /Probing system health.../i });
    expect(reprobeBtn).toBeInTheDocument();
    expect(reprobeBtn).toHaveAttribute('aria-busy', 'true');

    await act(async () => {
      resolveHealth({ success: true });
    });

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Re-run health probes/i })).toHaveAttribute('aria-busy', 'false');
    });
  });

  it('navigates to specified tabs when CTA buttons are clicked', async () => {
    const mockApi = {
      getHealth: vi.fn().mockResolvedValue({ success: true }),
      getModels: vi.fn().mockResolvedValue({ models: [] }),
      getInferenceEngines: vi.fn().mockResolvedValue({ engines: [] }),
    };
    const onNavigateToTab = vi.fn();

    render(<HealthPanel api={mockApi as any} onNavigateToTab={onNavigateToTab} />);

    const modelsBtn = await screen.findByRole('button', { name: 'Navigate to Models tab' });
    const settingsBtn = screen.getByRole('button', { name: 'Navigate to System Settings tab' });

    fireEvent.click(modelsBtn);
    expect(onNavigateToTab).toHaveBeenCalledWith('models');

    fireEvent.click(settingsBtn);
    expect(onNavigateToTab).toHaveBeenCalledWith('settings');
  });

  it('responds to retry-health-check custom event', async () => {
    const mockApi = {
      getHealth: vi.fn().mockResolvedValue({ success: true }),
      getModels: vi.fn().mockResolvedValue({ models: [] }),
      getInferenceEngines: vi.fn().mockResolvedValue({ engines: [] }),
    };

    render(<HealthPanel api={mockApi as any} />);

    await waitFor(() => expect(mockApi.getHealth).toHaveBeenCalledTimes(1));

    await act(async () => {
      window.dispatchEvent(new CustomEvent('retry-health-check'));
    });

    await waitFor(() => expect(mockApi.getHealth).toHaveBeenCalledTimes(2));
  });
});
