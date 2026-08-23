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

  it('saves the entered API key via api.setApiKey and shows confirmation', async () => {
    const api = createMockApi();
    render(<SecurityTab api={api} />);

    const input = screen.getByLabelText('API key');
    fireEvent.change(input, { target: { value: 'new-key-456' } });
    fireEvent.click(screen.getByRole('button', { name: /save/i }));

    expect(api.setApiKey).toHaveBeenCalledWith('new-key-456');
    expect(await screen.findByText('Saved')).toBeInTheDocument();
  });

  it('shows a restart-required message rather than claiming PQC is already active', async () => {
    const api = createMockApi();
    render(<SecurityTab api={api} />);

    fireEvent.click(await screen.findByRole('button', { name: /enable https \+ pqc-hybrid tls/i }));

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
    fireEvent.click(screen.getByRole('button', { name: /refresh token/i }));
    await screen.findByRole('button', { name: /copy access token/i });

    const copyBtn = screen.getByRole('button', { name: /copy access token/i });
    fireEvent.click(copyBtn);

    expect(writeTextMock).toHaveBeenCalledWith('a.b.c');
    expect(await screen.findByRole('button', { name: /copied token/i })).toBeInTheDocument();
  });
});
