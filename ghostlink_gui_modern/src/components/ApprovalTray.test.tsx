import React from 'react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { ApprovalTray } from './ApprovalTray';
import { GhostlinkAPI } from '../api';
import { PendingApproval } from '../store';

function approval(overrides: Partial<PendingApproval> = {}): PendingApproval {
  return {
    id: 'a1',
    workspace_id: 'ws_1',
    turn_id: 'sess_1',
    tool: 'write_file',
    server: 'filesystem',
    class: 'write',
    preview: 'write_file on notes.md',
    status: 'pending',
    created_at: 1,
    resolved_at: null,
    ...overrides,
  };
}

function makeApi(overrides: Partial<GhostlinkAPI> = {}): GhostlinkAPI {
  return {
    listApprovals: vi.fn().mockResolvedValue({ approvals: [] }),
    decideApproval: vi.fn().mockResolvedValue({ success: true, status: 'approved' }),
    ...overrides,
  } as unknown as GhostlinkAPI;
}

describe('ApprovalTray', () => {
  beforeEach(() => vi.useRealTimers());
  afterEach(() => vi.restoreAllMocks());

  it('renders nothing when no approvals are queued', async () => {
    const api = makeApi();
    const { container } = render(<ApprovalTray api={api} />);
    await waitFor(() => expect(api.listApprovals).toHaveBeenCalled());
    expect(container.querySelector('.approval-tray')).toBeNull();
  });

  it('lists a pending approval with its class, preview, and accessible action buttons', async () => {
    const api = makeApi({
      listApprovals: vi.fn().mockResolvedValue({ approvals: [approval()] }),
    } as unknown as Partial<GhostlinkAPI>);
    render(<ApprovalTray api={api} />);

    await screen.findByText('write_file');
    expect(screen.getByText('Write')).toBeTruthy();
    expect(screen.getByText('write_file on notes.md')).toBeTruthy();

    const approveBtn = screen.getByRole('button', { name: 'Approve write_file' });
    expect(approveBtn).toHaveAttribute('title', 'Approve write_file execution');
    expect(approveBtn).toHaveAttribute('aria-busy', 'false');
    expect(approveBtn.className).toContain('focus-visible:ring-2');

    const approveSessionBtn = screen.getByRole('button', { name: 'Approve write_file for session' });
    expect(approveSessionBtn).toHaveAttribute('title', 'Approve write_file for current session');
    expect(approveSessionBtn.className).toContain('focus-visible:ring-2');

    const denyBtn = screen.getByRole('button', { name: 'Deny write_file' });
    expect(denyBtn).toHaveAttribute('title', 'Deny write_file execution');
    expect(denyBtn.className).toContain('focus-visible:ring-2');

    const showResolvedCheckbox = screen.getByRole('checkbox', { name: 'Show resolved approvals' });
    expect(showResolvedCheckbox).toHaveAttribute('title', 'Toggle showing resolved approvals');
    expect(showResolvedCheckbox.className).toContain('focus-visible:ring-2');
  });

  it('never offers a session grant for an exec-class tool', async () => {
    // "Always allow" on a command runner is a standing shell. The backend
    // refuses it too; the button is hidden so the user isn't offered a dead end.
    const api = makeApi({
      listApprovals: vi
        .fn()
        .mockResolvedValue({ approvals: [approval({ class: 'exec', tool: 'run_command' })] }),
    } as unknown as Partial<GhostlinkAPI>);
    render(<ApprovalTray api={api} />);

    await screen.findByText('run_command');
    expect(screen.queryByRole('button', { name: /Approve run_command for session/ })).toBeNull();
    expect(screen.getByRole('button', { name: 'Approve run_command' })).toBeTruthy();
  });

  it('offers a session grant for a write-class tool', async () => {
    const api = makeApi({
      listApprovals: vi.fn().mockResolvedValue({ approvals: [approval()] }),
    } as unknown as Partial<GhostlinkAPI>);
    render(<ApprovalTray api={api} />);
    await screen.findByText('write_file');
    expect(screen.getByRole('button', { name: 'Approve write_file for session' })).toBeTruthy();
  });

  it('sends approve=true with the session flag when requested', async () => {
    const decide = vi.fn().mockResolvedValue({ success: true, status: 'approved' });
    const api = makeApi({
      listApprovals: vi.fn().mockResolvedValue({ approvals: [approval()] }),
      decideApproval: decide,
    } as unknown as Partial<GhostlinkAPI>);
    render(<ApprovalTray api={api} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Approve write_file for session' }));
    await waitFor(() =>
      expect(decide).toHaveBeenCalledWith('a1', true, { approveForSession: true })
    );
  });

  it('sends approve=false when denied', async () => {
    const decide = vi.fn().mockResolvedValue({ success: true, status: 'denied' });
    const api = makeApi({
      listApprovals: vi.fn().mockResolvedValue({ approvals: [approval()] }),
      decideApproval: decide,
    } as unknown as Partial<GhostlinkAPI>);
    render(<ApprovalTray api={api} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Deny write_file' }));
    await waitFor(() =>
      expect(decide).toHaveBeenCalledWith('a1', false, { approveForSession: false })
    );
  });

  it('shows the executed result after approving', async () => {
    const decide = vi.fn().mockResolvedValue({
      success: true,
      status: 'approved',
      result: 'wrote 12 bytes',
    });
    // The tray re-fetches after deciding, so the mock has to reflect the
    // resolved state the real backend would return on the next poll.
    const listApprovals = vi
      .fn()
      .mockResolvedValueOnce({ approvals: [approval()] })
      .mockResolvedValue({ approvals: [approval({ status: 'approved', resolved_at: 2 })] });
    const api = makeApi({ listApprovals, decideApproval: decide } as unknown as Partial<GhostlinkAPI>);
    render(<ApprovalTray api={api} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Approve write_file' }));
    // Approving should not be a blind act — the user sees what happened.
    expect(await screen.findByText('wrote 12 bytes')).toBeTruthy();
  });

  it('surfaces a backend error instead of failing silently', async () => {
    const api = makeApi({
      listApprovals: vi.fn().mockResolvedValue({ approvals: [approval()] }),
      decideApproval: vi.fn().mockResolvedValue({ success: false, error: 'server exploded' }),
    } as unknown as Partial<GhostlinkAPI>);
    render(<ApprovalTray api={api} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Approve write_file' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('server exploded');
  });

  it('tracks targeted action busy state so only the triggered decision button shows spinner and aria-busy', async () => {
    let resolveDecide!: (value: any) => void;
    const decidePromise = new Promise((resolve) => {
      resolveDecide = resolve;
    });

    const api = makeApi({
      listApprovals: vi.fn().mockResolvedValue({ approvals: [approval()] }),
      decideApproval: vi.fn().mockReturnValue(decidePromise),
    } as unknown as Partial<GhostlinkAPI>);

    render(<ApprovalTray api={api} />);

    const approveBtn = await screen.findByRole('button', { name: 'Approve write_file' });
    const approveSessionBtn = screen.getByRole('button', { name: 'Approve write_file for session' });
    const denyBtn = screen.getByRole('button', { name: 'Deny write_file' });

    fireEvent.click(approveBtn);

    // Approve button becomes aria-busy="true" and shows loading text
    expect(screen.getByRole('button', { name: 'Approving write_file...' })).toHaveAttribute('aria-busy', 'true');
    expect(approveSessionBtn).toHaveAttribute('aria-busy', 'false');
    expect(denyBtn).toHaveAttribute('aria-busy', 'false');

    // All buttons for this approval item should be disabled
    expect(screen.getByRole('button', { name: 'Approving write_file...' })).toBeDisabled();
    expect(approveSessionBtn).toBeDisabled();
    expect(denyBtn).toBeDisabled();

    // Resolve promise
    resolveDecide({ success: true, status: 'approved' });
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Approving write_file...' })).toBeNull());
  });

  it('shows resolved approvals without decision buttons', async () => {
    const api = makeApi({
      listApprovals: vi
        .fn()
        .mockResolvedValue({ approvals: [approval({ status: 'denied', resolved_at: 2 })] }),
    } as unknown as Partial<GhostlinkAPI>);
    render(<ApprovalTray api={api} />);

    await screen.findByText('Denied');
    expect(screen.queryByRole('button', { name: /Approve/ })).toBeNull();
    expect(screen.queryByRole('button', { name: /Deny/ })).toBeNull();
  });
});