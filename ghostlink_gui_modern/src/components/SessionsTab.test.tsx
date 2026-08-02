import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { SessionsTab } from './SessionsTab';
import { useAppStore } from '../store';

function createMockApi() {
  return {
    getSessions: vi.fn().mockResolvedValue({
      sessions: [
        { id: 'session-1', model: 'llama-3-8b', status: 'Running', throughput: 24.5, latency: 120, tokens: 412 },
        { id: 'session-2', model: 'mistral-7b', status: 'Idle', throughput: 0, latency: 0, tokens: 88 },
      ],
    }),
    cancelSession: vi.fn().mockResolvedValue({ success: true }),
  };
}

describe('SessionsTab', () => {
  beforeEach(() => {
    useAppStore.setState({
      sessions: [
        { id: 'session-1', model: 'llama-3-8b', status: 'Running', throughput: 24.5, latency: 120, tokens: 412 },
        { id: 'session-2', model: 'mistral-7b', status: 'Idle', throughput: 0, latency: 0, tokens: 88 },
      ],
      setSessions: vi.fn((sessions) => {
        useAppStore.setState({ sessions });
      }),
    });
    vi.restoreAllMocks();
  });

  it('renders sessions with custom tooltips and details', async () => {
    const api = createMockApi();
    render(<SessionsTab api={api} />);

    await waitFor(() => {
      expect(screen.getByText('Active Sessions')).toBeInTheDocument();
      expect(screen.getByText('session-1')).toBeInTheDocument();
      expect(screen.getByText('session-2')).toBeInTheDocument();
      expect(screen.getByText('llama-3-8b')).toBeInTheDocument();
      expect(screen.getByText('mistral-7b')).toBeInTheDocument();
    });

    // Check tooltip/title on Refresh button
    const refreshBtn = screen.getByRole('button', { name: /refresh sessions/i });
    expect(refreshBtn).toHaveAttribute('title', 'Refresh sessions');

    // Check tooltip/title on Cancel session buttons
    const cancelBtn1 = screen.getByRole('button', { name: /cancel session session-1/i });
    expect(cancelBtn1).toHaveAttribute('title', 'Cancel session session-1');
  });

  it('prompts confirm and calls cancelSession when confirmed', async () => {
    const api = createMockApi();
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);

    render(<SessionsTab api={api} />);

    await waitFor(() => {
      expect(screen.getByText('session-1')).toBeInTheDocument();
    });

    const cancelBtn = screen.getByRole('button', { name: /cancel session session-1/i });
    fireEvent.click(cancelBtn);

    expect(confirmSpy).toHaveBeenCalledWith(
      'Are you sure you want to cancel session session-1? This will immediately terminate the running inference.'
    );
    expect(api.cancelSession).toHaveBeenCalledWith('session-1');
  });

  it('does not call cancelSession when cancel is rejected in confirm dialog', async () => {
    const api = createMockApi();
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(false);

    render(<SessionsTab api={api} />);

    await waitFor(() => {
      expect(screen.getByText('session-1')).toBeInTheDocument();
    });

    const cancelBtn = screen.getByRole('button', { name: /cancel session session-1/i });
    fireEvent.click(cancelBtn);

    expect(confirmSpy).toHaveBeenCalled();
    expect(api.cancelSession).not.toHaveBeenCalled();
  });
});
