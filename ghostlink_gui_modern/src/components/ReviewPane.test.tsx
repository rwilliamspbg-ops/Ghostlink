import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ReviewPane } from './ReviewPane';
import { ReviewPacket } from '../api';

vi.mock('@monaco-editor/react', () => {
  const DiffEditor = ({ original, modified }: any) => {
    return (
      <div data-testid="mock-diff-editor">
        <div data-testid="mock-original">{original}</div>
        <div data-testid="mock-modified">{modified}</div>
      </div>
    );
  };
  return {
    __esModule: true,
    default: () => <div />,
    DiffEditor,
  };
});

describe('ReviewPane', () => {
  let mockPacket: ReviewPacket;
  let mockApi: any;

  beforeEach(() => {
    mockPacket = {
      id: 'rev_1234567890',
      task_id: 'task_1',
      run_id: 'run_1',
      summary: 'Propose new feature implementation',
      diffs: [
        {
          path: 'src/lib.rs',
          unified_diff: '--- a/src/lib.rs\n+++ b/src/lib.rs',
          original: 'fn old() {}',
          proposed: 'fn new_func() {}',
        },
      ],
      commands: [
        {
          argv: ['cargo', 'test'],
          judge: 'allow',
          exit: 0,
          excerpt: 'test result: ok',
        },
      ],
      checks: ['cargo test passed'],
      risks: ['None identified'],
      created_at: '2026-08-10T12:00:00Z',
    };

    mockApi = {
      decideReview: vi.fn().mockResolvedValue({ status: 'accepted', task_id: 'task_1' }),
    };
  });

  it('shows UNVERIFIED when no verification was run, not a pass', () => {
    render(<ReviewPane packet={mockPacket} api={mockApi} />);

    // mockPacket has no `verification` key at all -- the case that matters.
    const banner = screen.getByRole('status');
    expect(banner).toHaveTextContent(/Verification NOT run/i);
    expect(banner).toHaveTextContent(/UNVERIFIED/i);
    expect(banner.getAttribute('aria-label')).toBe(
      'Verification not run: this change is UNVERIFIED'
    );
    expect(screen.getByText(/No verification commands were run/i)).toBeInTheDocument();
  });

  it('reports a passing verification run', () => {
    render(
      <ReviewPane
        packet={{
          ...mockPacket,
          verification: [
            { argv: ['cargo', 'test'], exit: 0, passed: true, excerpt: 'test result: ok' },
          ],
        }}
        api={mockApi}
      />
    );

    const banner = screen.getByRole('status');
    expect(banner).toHaveTextContent(/Verification passed/i);
    expect(banner).toHaveClass('text-green-300');
  });

  it('reports a failing verification run and shows why it failed', () => {
    render(
      <ReviewPane
        packet={{
          ...mockPacket,
          verification: [
            { argv: ['cargo', 'test'], exit: 0, passed: true, excerpt: 'ok' },
            {
              argv: ['cargo', 'clippy'],
              exit: 101,
              passed: false,
              excerpt: 'error: unused variable `x`',
            },
          ],
        }}
        api={mockApi}
      />
    );

    const banner = screen.getByRole('status');
    expect(banner).toHaveTextContent(/Verification failed/i);
    expect(banner).toHaveClass('text-rose-300');
    // The excerpt is the point of capturing it -- a bare "FAIL" is not actionable.
    expect(screen.getByText(/unused variable/)).toBeInTheDocument();
    expect(screen.getByText('exit 101')).toBeInTheDocument();
  });

  it('marks a timed-out verification command as a timeout, not a plain failure', () => {
    render(
      <ReviewPane
        packet={{
          ...mockPacket,
          verification: [
            {
              argv: ['cargo', 'test'],
              exit: -1,
              passed: false,
              excerpt: '',
              timed_out: true,
            },
          ],
        }}
        api={mockApi}
      />
    );

    expect(screen.getByText('timeout')).toBeInTheDocument();
  });

  it('renders the checks list the backend sends', () => {
    render(<ReviewPane packet={mockPacket} api={mockApi} />);
    expect(screen.getByText('cargo test passed')).toBeInTheDocument();
  });

  it('explains the empty-diff state rather than showing a blank pane', () => {
    render(<ReviewPane packet={{ ...mockPacket, diffs: [] }} api={mockApi} />);

    // The sidebar count and the pane both say "No file changes proposed"; assert
    // on the heading with its count, and on the pane's added explanation.
    expect(screen.getByText(/Proposed Diffs \(0\)/)).toBeInTheDocument();
    expect(
      screen.getByText(/this run produced no diffs to review/i)
    ).toBeInTheDocument();
    expect(screen.queryByText(/Select a diff file to review/i)).not.toBeInTheDocument();
  });

  it('renders review packet summary and diff file path', () => {
    render(<ReviewPane packet={mockPacket} api={mockApi} />);

    expect(screen.getByText(/ReviewPacket #rev_1234/i)).toBeInTheDocument();
    expect(screen.getByText('Propose new feature implementation')).toBeInTheDocument();
    expect(screen.getByText('src/lib.rs')).toBeInTheDocument();
  });

  it('calls decideReview with accept when Accept button is clicked and shows loading state', async () => {
    let resolveReview: any;
    mockApi.decideReview.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveReview = resolve;
        })
    );
    const onDecided = vi.fn();
    render(<ReviewPane packet={mockPacket} api={mockApi} onDecided={onDecided} />);

    const acceptBtn = screen.getByRole('button', { name: /accept proposed changes/i });
    expect(acceptBtn).toHaveAttribute('aria-label', 'Accept proposed changes');
    expect(acceptBtn).toHaveAttribute('title', 'Accept proposed changes and apply to project root');

    fireEvent.click(acceptBtn);

    expect(acceptBtn).toBeDisabled();
    expect(acceptBtn).toHaveAttribute('aria-busy', 'true');
    expect(acceptBtn).toHaveAttribute('aria-label', 'Accepting proposed changes...');

    resolveReview({ status: 'accepted', task_id: 'task_1' });

    await waitFor(() => {
      expect(mockApi.decideReview).toHaveBeenCalledWith('rev_1234567890', {
        decision: 'accept',
        note: undefined,
      });
      expect(onDecided).toHaveBeenCalled();
    });
  });

  it('shows note input and calls decideReview with request_changes and note', async () => {
    const onDecided = vi.fn();
    render(<ReviewPane packet={mockPacket} api={mockApi} onDecided={onDecided} />);

    const reqBtn = screen.getByRole('button', { name: /request changes/i });
    fireEvent.click(reqBtn);

    const noteInput = screen.getByRole('textbox', { name: /note for requested changes/i });
    expect(noteInput).toBeInTheDocument();

    fireEvent.change(noteInput, { target: { value: 'Please update comments' } });
    fireEvent.click(reqBtn);

    await waitFor(() => {
      expect(mockApi.decideReview).toHaveBeenCalledWith('rev_1234567890', {
        decision: 'request_changes',
        note: 'Please update comments',
      });
      expect(onDecided).toHaveBeenCalled();
    });
  });

  it('calls decideReview with reject when Reject button is clicked', async () => {
    const onDecided = vi.fn();
    render(<ReviewPane packet={mockPacket} api={mockApi} onDecided={onDecided} />);

    const rejectBtn = screen.getByRole('button', { name: /reject proposed changes/i });
    fireEvent.click(rejectBtn);

    await waitFor(() => {
      expect(mockApi.decideReview).toHaveBeenCalledWith('rev_1234567890', {
        decision: 'reject',
        note: undefined,
      });
      expect(onDecided).toHaveBeenCalled();
    });
  });

  it('renders diff file selection button with accessible aria-label, title, and aria-current', () => {
    render(<ReviewPane packet={mockPacket} api={mockApi} />);

    const diffBtn = screen.getByRole('button', { name: /view diff for src\/lib.rs/i });
    expect(diffBtn).toBeInTheDocument();
    expect(diffBtn).toHaveAttribute('title', 'View diff for src/lib.rs');
    expect(diffBtn).toHaveAttribute('aria-current', 'true');
  });

  it('renders commands list with judge status badges', () => {
    render(<ReviewPane packet={mockPacket} api={mockApi} />);

    expect(screen.getByText('cargo test')).toBeInTheDocument();
    expect(screen.getByText('allow')).toBeInTheDocument();
  });
});
