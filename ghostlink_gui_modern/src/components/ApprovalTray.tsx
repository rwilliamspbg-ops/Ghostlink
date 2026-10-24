import React, { useCallback, useEffect, useState } from 'react';
import { Loader } from 'lucide-react';
import { GhostlinkAPI } from '../api';
import { PendingApproval } from '../store';

/** Human labels for the capability classes, so the tray says "Write" rather
 *  than "write". `exec` is called out explicitly because it is the class that
 *  can never be approved for the whole session. */
const CLASS_LABEL: Record<string, string> = {
  read: 'Read',
  write: 'Write',
  exec: 'Exec',
};

const STATUS_LABEL: Record<string, string> = {
  pending: 'Awaiting your decision',
  approved: 'Approved',
  edited: 'Approved (edited)',
  denied: 'Denied',
  approved_for_session: 'Approved for this session',
};

/** How often the tray re-checks for newly queued actions while one is open.
 *  Polling rather than a websocket: the queue changes only when a chat turn
 *  runs, and the endpoint is a cheap read of a bounded in-memory list. */
const POLL_MS = 5000;

type ApprovalAction = 'approve' | 'approve_session' | 'deny';

/**
 * Review tray for tool calls the capability gate held back.
 *
 * A gated call is not executed and the chat turn finishes normally — the model is
 * told an approval id and moves on. This tray is where that decision gets made.
 * Approving executes the action immediately and shows its result inline.
 */
export const ApprovalTray: React.FC<{ api: GhostlinkAPI }> = ({ api }) => {
  const [approvals, setApprovals] = useState<PendingApproval[]>([]);
  const [showResolved, setShowResolved] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [busyAction, setBusyAction] = useState<ApprovalAction | null>(null);
  const [results, setResults] = useState<Record<string, { ok: boolean; text: string }>>({});
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const res = await api.listApprovals({ all: showResolved });
    if (res.error) {
      setError(res.error);
      return;
    }
    setError(null);
    setApprovals(res.approvals);
  }, [api, showResolved]);

  useEffect(() => {
    void refresh();
    const id = setInterval(() => void refresh(), POLL_MS);
    return () => clearInterval(id);
  }, [refresh]);

  const decide = useCallback(
    async (
      approval: PendingApproval,
      approve: boolean,
      approveForSession = false,
      action: ApprovalAction = 'approve'
    ) => {
      setBusyId(approval.id);
      setBusyAction(action);
      setError(null);
      const res = await api.decideApproval(approval.id, approve, { approveForSession });
      setBusyId(null);
      setBusyAction(null);
      if (!res.success) {
        setError(res.error ?? 'approval failed');
        return;
      }
      // Surface what the approved action actually did, so approving is not a
      // blind act of faith.
      if (res.result !== undefined) {
        setResults((prev) => ({
          ...prev,
          [approval.id]: { ok: true, text: res.result as string },
        }));
      }
      await refresh();
    },
    [api, refresh]
  );

  const pending = approvals.filter((a) => a.status === 'pending');

  if (approvals.length === 0) {
    return null;
  }

  return (
    <section aria-label="Approval tray" className="approval-tray">
      <header className="approval-tray__header">
        <h3>
          Approvals
          {pending.length > 0 && (
            <span className="approval-tray__badge">{pending.length}</span>
          )}
        </h3>
        <label className="approval-tray__toggle">
          <input
            type="checkbox"
            checked={showResolved}
            onChange={(e) => setShowResolved(e.target.checked)}
            aria-label="Show resolved approvals"
            title="Toggle showing resolved approvals"
            className="focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
          />
          Show resolved
        </label>
      </header>

      {error && (
        <p role="alert" className="approval-tray__error">
          {error}
        </p>
      )}

      <ul className="approval-tray__list">
        {approvals.map((a) => {
          const isItemBusy = busyId === a.id;
          const isApproveBusy = isItemBusy && busyAction === 'approve';
          const isSessionBusy = isItemBusy && busyAction === 'approve_session';
          const isDenyBusy = isItemBusy && busyAction === 'deny';
          const result = results[a.id];
          return (
            <li key={a.id} className={`approval-tray__item approval-tray__item--${a.status}`}>
              <div className="approval-tray__meta">
                <code>{a.tool}</code>
                <span className={`approval-tray__class approval-tray__class--${a.class}`}>
                  {CLASS_LABEL[a.class] ?? a.class}
                </span>
                <span className="approval-tray__status">{STATUS_LABEL[a.status] ?? a.status}</span>
              </div>

              {/* The preview is a short, bounded description of the intended
                  effect — never the full argument bag, which is where secrets
                  would appear. */}
              <p className="approval-tray__preview">{a.preview}</p>

              {a.status === 'pending' ? (
                <div className="approval-tray__actions">
                  <button
                    type="button"
                    disabled={isItemBusy}
                    aria-busy={isApproveBusy}
                    aria-label={isApproveBusy ? `Approving ${a.tool}...` : `Approve ${a.tool}`}
                    title={isApproveBusy ? `Approving ${a.tool}...` : `Approve ${a.tool} execution`}
                    className="focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                    onClick={() => void decide(a, true, false, 'approve')}
                  >
                    {isApproveBusy ? (
                      <span className="inline-flex items-center gap-1">
                        <Loader size={14} className="animate-spin" aria-hidden="true" />
                        Approving...
                      </span>
                    ) : (
                      'Approve'
                    )}
                  </button>
                  {/* Session grants are never offered for exec: an "always allow"
                      on a command runner is a standing shell. The backend refuses
                      it too, so hiding the button just avoids a dead end. */}
                  {a.class !== 'exec' && (
                    <button
                      type="button"
                      disabled={isItemBusy}
                      aria-busy={isSessionBusy}
                      aria-label={isSessionBusy ? `Approving ${a.tool} for session...` : `Approve ${a.tool} for session`}
                      title={isSessionBusy ? `Approving ${a.tool} for session...` : `Approve ${a.tool} for current session`}
                      className="focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                      onClick={() => void decide(a, true, true, 'approve_session')}
                    >
                      {isSessionBusy ? (
                        <span className="inline-flex items-center gap-1">
                          <Loader size={14} className="animate-spin" aria-hidden="true" />
                          Approving...
                        </span>
                      ) : (
                        'Approve for session'
                      )}
                    </button>
                  )}
                  <button
                    type="button"
                    disabled={isItemBusy}
                    aria-busy={isDenyBusy}
                    aria-label={isDenyBusy ? `Denying ${a.tool}...` : `Deny ${a.tool}`}
                    title={isDenyBusy ? `Denying ${a.tool}...` : `Deny ${a.tool} execution`}
                    className="approval-tray__deny focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                    onClick={() => void decide(a, false, false, 'deny')}
                  >
                    {isDenyBusy ? (
                      <span className="inline-flex items-center gap-1">
                        <Loader size={14} className="animate-spin" aria-hidden="true" />
                        Denying...
                      </span>
                    ) : (
                      'Deny'
                    )}
                  </button>
                </div>
              ) : (
                result && (
                  <p className={`approval-tray__result ${result.ok ? '' : 'is-error'}`}>
                    {result.text}
                  </p>
                )
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}