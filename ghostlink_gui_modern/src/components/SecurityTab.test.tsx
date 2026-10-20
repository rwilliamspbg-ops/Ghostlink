import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { SecurityTab } from './SecurityTab';

function createMockApi(overrides: Partial<ReturnType<typeof baseMock>> = {}) {
  return { ...baseMock(), ...overrides };
}

function baseMock() {
  let storedKey = '';
  return {
    getApiKey: vi.fn(() => storedKey),
    setApiKey: vi.fn((key: string) => {
      storedKey = key;
    }),
    getPQCState: vi.fn().mockResolvedValue({ enabled: false }),
    enablePQC: vi.fn().mockResolvedValue({ success: true, data: { restart_required: true, enabled: true } }),
    getAuditLog: vi.fn().mockResolvedValue({ entries: [] }),
    refreshJWT: vi.fn().mockResolvedValue({ success: true, data: { token: 'a.b.c' } }),
  };
}

describe('SecurityTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('loads the currently-stored API key into the input on mount', async () => {
    const api = createMockApi();
    api.getApiKey.mockReturnValue('existing-key-123');

    render(<SecurityTab api={api} />);

    await waitFor(() => {
      const input = screen.getByLabelText('API key') as HTMLInputElement;
      expect(input.value).toBe('existing-key-123');
    });
  });

  it('saves the entered API key via api.setApiKey and shows confirmation with ARIA label', async () => {
    const api = createMockApi();
    render(<SecurityTab api={api} />);

    const input = screen.getByLabelText('API key');
    fireEvent.change(input, { target: { value: 'new-key-456' } });
    const saveBtn = screen.getByRole('button', { name: /save api key/i });
    fireEvent.click(saveBtn);

    expect(api.setApiKey).toHaveBeenCalledWith('new-key-456');
    expect(await screen.findByRole('button', { name: /api key saved/i })).toBeInTheDocument();
    expect(screen.getByText('Saved')).toBeInTheDocument();
  });

  it('saves the entered API key when submitting the form via Enter keypress', async () => {
    const api = createMockApi();
    render(<SecurityTab api={api} />);

    const input = screen.getByLabelText('API key');
    fireEvent.change(input, { target: { value: 'enter-key-789' } });
    fireEvent.submit(input);

    expect(api.setApiKey).toHaveBeenCalledWith('enter-key-789');
    expect(await screen.findByRole('button', { name: /api key saved/i })).toBeInTheDocument();
  });

  it('handles audit log refresh button loading state and aria-busy attribute', async () => {
    let resolveAudit: any;
    const auditPromise = new Promise((resolve) => { resolveAudit = resolve; });
    const api = createMockApi({
      getAuditLog: vi.fn().mockImplementation(() => auditPromise),
    });

    render(<SecurityTab api={api} />);

    const refreshBtn = screen.getByRole('button', { name: 'Refresh audit log' });
    fireEvent.click(refreshBtn);

    expect(refreshBtn).toHaveAttribute('aria-busy', 'true');
    expect(screen.getByRole('button', { name: 'Refreshing audit log...' })).toBeInTheDocument();

    resolveAudit({ entries: [{ event: 'TEST_EVENT', status: 'SUCCESS', ip: '127.0.0.1', time: '2026-09-11T00:00:00Z' }] });

    await waitFor(() => {
      expect(refreshBtn).toHaveAttribute('aria-busy', 'false');
      expect(screen.getByText('TEST_EVENT')).toBeInTheDocument();
    });
  });

  it('handles token refresh button loading state and aria-busy attribute', async () => {
    let resolveRefresh: any;
    const refreshPromise = new Promise((resolve) => { resolveRefresh = resolve; });
    const api = createMockApi({
      refreshJWT: vi.fn().mockImplementation(() => refreshPromise),
    });

    render(<SecurityTab api={api} />);

    const refreshBtn = screen.getByRole('button', { name: 'Refresh JWT access token' });
    expect(refreshBtn).toHaveAttribute('aria-busy', 'false');
    expect(refreshBtn).toHaveAttribute('title', 'Refresh JWT access token');

    fireEvent.click(refreshBtn);

    expect(refreshBtn).toHaveAttribute('aria-busy', 'true');
    expect(screen.getByRole('button', { name: 'Refreshing JWT access token...' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Refreshing JWT access token...' })).toHaveAttribute('title', 'Refreshing JWT access token...');
    expect(screen.getByText('Refreshing Token...')).toBeInTheDocument();

    resolveRefresh({ success: true, data: { token: 'new.jwt.token' } });

    await waitFor(() => {
      expect(refreshBtn).toHaveAttribute('aria-busy', 'false');
      expect(screen.getByText('Refresh Token')).toBeInTheDocument();
    });
  });

  it('handles PQC enable button loading state and aria-busy attribute', async () => {
    let resolvePqc: any;
    const pqcPromise = new Promise((resolve) => { resolvePqc = resolve; });
    const api = createMockApi({
      enablePQC: vi.fn().mockImplementation(() => pqcPromise),
    });

    render(<SecurityTab api={api} />);

    const enableBtn = await screen.findByRole('button', { name: 'Enable HTTPS and PQC-hybrid TLS' });
    expect(enableBtn).toHaveAttribute('aria-busy', 'false');
    expect(enableBtn).toHaveAttribute('title', 'Enable HTTPS and PQC-hybrid TLS');

    fireEvent.click(enableBtn);

    expect(screen.getByRole('button', { name: 'Enabling HTTPS and PQC-hybrid TLS...' })).toBeInTheDocument();
    const busyBtn = screen.getByRole('button', { name: 'Enabling HTTPS and PQC-hybrid TLS...' });
    expect(busyBtn).toHaveAttribute('aria-busy', 'true');
    expect(busyBtn).toHaveAttribute('title', 'Enabling HTTPS and PQC-hybrid TLS...');
    expect(screen.getByText('Enabling HTTPS + PQC-Hybrid TLS...')).toBeInTheDocument();

    resolvePqc({ success: true, data: { restart_required: true, enabled: true } });

    await waitFor(() => {
      expect(screen.getByText(/restart the server/i)).toBeInTheDocument();
    });
  });

  it('shows a restart-required message rather than claiming PQC is already active', async () => {
    const api = createMockApi();
    render(<SecurityTab api={api} />);

    fireEvent.click(await screen.findByRole('button', { name: /enable https and pqc-hybrid tls/i }));

    await waitFor(() => {
      expect(screen.getByText(/restart the server/i)).toBeInTheDocument();
    });
    // Must not claim the hardened state is already live.
    expect(screen.queryByText(/HTTPS \+ PQC-hybrid active/i)).not.toBeInTheDocument();
  });

  it('reflects a real enabled PQC state from the backend without requiring a click', async () => {
    const api = createMockApi({ getPQCState: vi.fn().mockResolvedValue({ enabled: true }) });
    render(<SecurityTab api={api} />);

    expect(await screen.findByText(/HTTPS \+ PQC-hybrid active/i)).toBeInTheDocument();
  });

  it('copies the access token to clipboard when copy button is clicked', async () => {
    const api = createMockApi();
    const writeTextMock = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText: writeTextMock } });

    render(<SecurityTab api={api} />);

    // Refresh token first to have a valid token
    fireEvent.click(screen.getByRole('button', { name: /refresh jwt access token/i }));
    await screen.findByRole('button', { name: /copy access token/i });

    const copyBtn = screen.getByRole('button', { name: /copy access token/i });
    fireEvent.click(copyBtn);

    expect(writeTextMock).toHaveBeenCalledWith('a.b.c');
    expect(await screen.findByRole('button', { name: /copied token/i })).toBeInTheDocument();
  });

  it('renders recovery API key form with proper ARIA labels and disabled state when health check returns 401', async () => {
    const api = createMockApi({
      getHealth: vi.fn().mockResolvedValue({ success: false, error: '401' }),
      getModels: vi.fn().mockResolvedValue({ error: '401' }),
      getInferenceEngines: vi.fn().mockRejectedValue({ response: { status: 401 } }),
    });

    render(<SecurityTab api={api} />);

    const recoveryInput = await screen.findByLabelText(/recovery api key input/i);
    const applyButton = screen.getByRole('button', { name: /apply recovery api key/i });

    expect(recoveryInput).toBeInTheDocument();
    expect(recoveryInput).toHaveAttribute('title', 'Enter Bearer API key for HTTP 401 recovery');
    expect(applyButton).toBeInTheDocument();
    expect(applyButton).toBeDisabled();
    expect(applyButton).toHaveAttribute('title', 'Enter an API key to apply');

    fireEvent.change(recoveryInput, { target: { value: 'recovery-key-123' } });
    expect(applyButton).not.toBeDisabled();
    expect(applyButton).toHaveAttribute('title', 'Apply recovery API key');
  });
});
