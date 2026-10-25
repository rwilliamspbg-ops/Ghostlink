import { resolveApiBase } from '../config';
import React, { useState } from 'react';
import { DiffEditor } from '@monaco-editor/react';
import { Check, X, RefreshCw, AlertTriangle, Terminal, Shield, FileText, Cpu, Layers } from 'lucide-react';
import { ReviewPacket, ReviewDiff, VerificationResult, GhostlinkAPI } from '../api';
import { useAppStore } from '../store';

function guessLanguage(path: string): string {
  const ext = path.split('.').pop()?.toLowerCase();
  switch (ext) {
    case 'rs':
      return 'rust';
    case 'ts':
    case 'tsx':
      return 'typescript';
    case 'js':
    case 'jsx':
      return 'javascript';
    case 'py':
      return 'python';
    case 'json':
      return 'json';
    case 'html':
      return 'html';
    case 'css':
      return 'css';
    case 'md':
      return 'markdown';
    case 'go':
      return 'go';
    case 'sh':
      return 'shell';
    case 'toml':
      return 'toml';
    case 'yaml':
    case 'yml':
      return 'yaml';
    default:
      return 'plaintext';
  }
}

interface ReviewPaneProps {
  packet: ReviewPacket;
  api?: GhostlinkAPI;
  onDecided?: () => void;
}

export const ReviewPane: React.FC<ReviewPaneProps> = ({ packet, api: propApi, onDecided }) => {
  const api = propApi || new GhostlinkAPI(resolveApiBase());

  const [selectedDiffIndex, setSelectedDiffIndex] = useState<number>(0);
  const [activeAction, setActiveAction] = useState<'accept' | 'request_changes' | 'reject' | null>(null);
  const [note, setNote] = useState<string>('');
  const [showNoteInput, setShowNoteInput] = useState<boolean>(false);

  const addToast = useAppStore((state) => state.addToast);
  const currentDiff: ReviewDiff | undefined = packet.diffs[selectedDiffIndex];

  // Tri-state, matching the backend's `verification_passed() -> Option<bool>`:
  // undefined means no verification was run at all, which is NOT a pass. Rendering
  // that as green is how an unverified change gets accepted.
  const verification: VerificationResult[] = packet.verification ?? [];
  const verificationRan = verification.length > 0;
  const verificationPassed = verificationRan && verification.every((v) => v.passed);
  const verificationFailed = verificationRan && !verificationPassed;

  const handleDecide = async (decision: 'accept' | 'request_changes' | 'reject') => {
    setActiveAction(decision);
    try {
      await api.decideReview(packet.id, { decision, note: note.trim() || undefined });
      addToast({
        message: `Review decision '${decision}' applied successfully.`,
        type: 'success',
      });
      if (onDecided) onDecided();
    } catch (err: any) {
      addToast({
        message: err.response?.data?.error || err.message || 'Failed to submit decision',
        type: 'error',
      });
    } finally {
      setActiveAction(null);
    }
  };

  const isDeciding = activeAction !== null;

  return (
    <div className="flex flex-col h-full bg-slate-900 border border-slate-800 rounded-xl overflow-hidden shadow-2xl">
      {/* Header */}
      <div className="px-6 py-4 bg-slate-950 border-b border-slate-800 flex flex-wrap items-center justify-between gap-4">
        <div>
          <div className="flex items-center gap-2">
            <Shield className="w-5 h-5 text-indigo-400" aria-hidden="true" />
            <h2 className="text-lg font-bold text-slate-100">ReviewPacket #{packet.id.slice(0, 8)}</h2>
          </div>
          <p className="text-sm text-slate-400 mt-1">{packet.summary}</p>
          {/* Verification verdict, surfaced at the top rather than in the sidebar.
              The three states are deliberately distinct: "not run" must never read
              as "passed". */}
          <div
            className={`mt-2 inline-flex items-center gap-1.5 px-2 py-1 rounded-lg text-xs font-bold ${
              verificationPassed
                ? 'bg-green-500/15 text-green-300 border border-green-500/30'
                : verificationFailed
                ? 'bg-rose-500/15 text-rose-300 border border-rose-500/30'
                : 'bg-amber-500/15 text-amber-300 border border-amber-500/30'
            }`}
            role="status"
            aria-label={
              verificationPassed
                ? `Verification passed: ${verification.length} command(s)`
                : verificationFailed
                ? `Verification failed: ${verification.filter((v) => !v.passed).length} of ${verification.length} command(s) failed`
                : 'Verification not run: this change is UNVERIFIED'
            }
          >
            {verificationPassed ? (
              <Check size={13} aria-hidden="true" />
            ) : verificationFailed ? (
              <X size={13} aria-hidden="true" />
            ) : (
              <AlertTriangle size={13} aria-hidden="true" />
            )}
            {verificationPassed
              ? `Verification passed (${verification.length})`
              : verificationFailed
              ? `Verification failed (${verification.filter((v) => !v.passed).length}/${verification.length})`
              : 'Verification NOT run — UNVERIFIED'}
          </div>
        </div>

        {/* Action Buttons */}
        <div className="flex items-center gap-2">
          {showNoteInput && (
            <input
              type="text"
              placeholder="Note for request changes..."
              aria-label="Note for requested changes"
              value={note}
              onChange={(e) => setNote(e.target.value)}
              className="px-3 py-1.5 bg-slate-900 border border-slate-700 rounded-lg text-xs text-slate-200 focus:outline-none focus:ring-2 focus:ring-amber-500 focus-visible:ring-2 focus-visible:ring-amber-500 focus-visible:outline-none"
            />
          )}
          <button
            onClick={() => handleDecide('accept')}
            disabled={isDeciding}
            aria-busy={activeAction === 'accept'}
            aria-label={activeAction === 'accept' ? 'Accepting proposed changes...' : 'Accept proposed changes'}
            title={
              activeAction === 'accept'
                ? 'Accepting proposed changes...'
                : 'Accept proposed changes and apply to project root'
            }
            className="flex items-center gap-1.5 px-4 py-2 rounded-lg text-xs font-bold bg-green-600 hover:bg-green-500 text-white transition disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-green-500 focus-visible:outline-none"
          >
            {activeAction === 'accept' ? (
              <RefreshCw size={14} className="animate-spin" aria-hidden="true" />
            ) : (
              <Check size={14} aria-hidden="true" />
            )}
            Accept
          </button>
          <button
            onClick={() => {
              if (!showNoteInput) {
                setShowNoteInput(true);
              } else {
                handleDecide('request_changes');
              }
            }}
            disabled={isDeciding}
            aria-busy={activeAction === 'request_changes'}
            aria-label={
              activeAction === 'request_changes' ? 'Requesting changes...' : 'Request changes'
            }
            title={
              activeAction === 'request_changes'
                ? 'Requesting changes...'
                : 'Request changes and requeue task with note'
            }
            className="flex items-center gap-1.5 px-4 py-2 rounded-lg text-xs font-bold bg-amber-600 hover:bg-amber-500 text-white transition disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-amber-500 focus-visible:outline-none"
          >
            <RefreshCw
              size={14}
              className={activeAction === 'request_changes' ? 'animate-spin' : ''}
              aria-hidden="true"
            />
            Request Changes
          </button>
          <button
            onClick={() => handleDecide('reject')}
            disabled={isDeciding}
            aria-busy={activeAction === 'reject'}
            aria-label={activeAction === 'reject' ? 'Rejecting proposed changes...' : 'Reject proposed changes'}
            title={
              activeAction === 'reject'
                ? 'Rejecting proposed changes...'
                : 'Reject proposed changes and delete staging tree'
            }
            className="flex items-center gap-1.5 px-4 py-2 rounded-lg text-xs font-bold bg-rose-700 hover:bg-rose-600 text-white transition disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-rose-500 focus-visible:outline-none"
          >
            {activeAction === 'reject' ? (
              <RefreshCw size={14} className="animate-spin" aria-hidden="true" />
            ) : (
              <X size={14} aria-hidden="true" />
            )}
            Reject
          </button>
        </div>
      </div>

      {/* Main Content split: Left diffs list/details, Right diff view */}
      <div className="flex-1 flex min-h-0">
        {/* Left Sidebar: Diffs list, commands, risks */}
        <div className="w-80 border-r border-slate-800 bg-slate-950 flex flex-col overflow-y-auto">
          {/* File Diffs List */}
          <div className="p-4 border-b border-slate-800">
            <h3 className="text-xs font-semibold uppercase text-slate-400 mb-2 flex items-center gap-1.5">
              <FileText size={14} aria-hidden="true" /> Proposed Diffs ({packet.diffs.length})
            </h3>
            {packet.diffs.length === 0 ? (
              <p className="text-xs text-slate-500 italic">No file changes proposed</p>
            ) : (
              <div className="space-y-1">
                {packet.diffs.map((diff, idx) => (
                  <button
                    key={diff.path}
                    onClick={() => setSelectedDiffIndex(idx)}
                    aria-current={selectedDiffIndex === idx ? 'true' : undefined}
                    aria-label={`View diff for ${diff.path}`}
                    title={`View diff for ${diff.path}`}
                    className={`w-full text-left px-3 py-2 rounded-lg text-xs font-mono transition flex items-center justify-between focus-visible:ring-2 focus-visible:ring-indigo-500 focus-visible:outline-none ${
                      selectedDiffIndex === idx
                        ? 'bg-indigo-600/20 text-indigo-300 border border-indigo-500/30'
                        : 'text-slate-300 hover:bg-slate-900'
                    }`}
                  >
                    <span className="truncate">{diff.path}</span>
                  </button>
                ))}
              </div>
            )}
          </div>

          {/* Verification detail */}
          <div className="p-4 border-b border-slate-800">
            <h3 className="text-xs font-semibold uppercase text-slate-400 mb-2 flex items-center gap-1.5">
              <Shield size={14} aria-hidden="true" /> Verification
            </h3>
            {!verificationRan ? (
              <p className="text-xs text-amber-400/90 italic">
                No verification commands were run for this change.
              </p>
            ) : (
              <div className="space-y-2">
                {verification.map((v, idx) => (
                  <div
                    key={idx}
                    className={`p-2 rounded border text-xs ${
                      v.passed
                        ? 'bg-green-500/5 border-green-500/20'
                        : 'bg-rose-500/5 border-rose-500/20'
                    }`}
                  >
                    <div className="flex items-center justify-between gap-2">
                      <span className="font-mono text-slate-200 truncate" title={v.argv.join(' ')}>
                        {v.argv.join(' ')}
                      </span>
                      <span
                        className={`text-[10px] px-1.5 py-0.5 rounded font-bold uppercase shrink-0 ${
                          v.passed ? 'bg-green-500/20 text-green-400' : 'bg-rose-500/20 text-rose-400'
                        }`}
                      >
                        {v.timed_out ? 'timeout' : v.passed ? 'pass' : `exit ${v.exit}`}
                      </span>
                    </div>
                    {/* The excerpt is why the backend captured it: a bare
                        "FAIL" does not tell a reviewer what to fix. */}
                    {!v.passed && v.excerpt && (
                      <pre className="mt-1.5 p-1.5 bg-slate-950 rounded text-[10px] text-slate-400 overflow-x-auto max-h-24 whitespace-pre-wrap">
                        {v.excerpt}
                      </pre>
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* Placement and accelerator usage. Only rendered when the backend
              actually ran on a cluster -- `placement` is absent (not null) in
              single-node mode, so this section simply does not appear there
              rather than claiming an unknown node. */}
          {packet.placement && (
            <div className="p-4 border-b border-slate-800">
              <h3 className="text-xs font-semibold uppercase text-slate-400 mb-2 flex items-center gap-1.5">
                <Cpu size={14} aria-hidden="true" /> Placement
              </h3>
              <p className="text-xs text-slate-300">
                <span className="font-mono text-indigo-300">{packet.placement.assigned_node}</span>
                {' — '}
                {packet.placement.node_capability.compute_capability.toUpperCase()}
                {packet.placement.node_capability.gpu_name
                  ? ` (${packet.placement.node_capability.gpu_name})`
                  : ''}
                , {packet.placement.node_capability.available_vram_gb.toFixed(1)}GB VRAM free
              </p>
              <p className="text-xs text-slate-500 mt-1">{packet.placement.rationale}</p>
              {packet.accelerator_usage && (
                <p className="text-xs text-slate-500 mt-1">
                  {packet.accelerator_usage.commands_executed} command(s),{' '}
                  {packet.accelerator_usage.tokens_generated} token(s) generated on this node.
                </p>
              )}
            </div>
          )}

          {/* Context compaction. Present only when turns were actually
              dropped; its absence is the signal that nothing was lost. */}
          {packet.context_summary && (
            <div className="p-4 border-b border-slate-800">
              <h3 className="text-xs font-semibold uppercase text-slate-400 mb-2 flex items-center gap-1.5">
                <Layers size={14} aria-hidden="true" /> Context Compacted
              </h3>
              <p className="text-xs text-amber-300/90">{packet.context_summary.summary_text}</p>
              {packet.context_summary.files_modified.length > 0 && (
                <p className="text-xs text-slate-500 mt-1">
                  Files touched before compaction:{' '}
                  <span className="font-mono">
                    {packet.context_summary.files_modified.join(', ')}
                  </span>
                </p>
              )}
            </div>
          )}

          {/* Checks */}
          {packet.checks && packet.checks.length > 0 && (
            <div className="p-4 border-b border-slate-800">
              <h3 className="text-xs font-semibold uppercase text-slate-400 mb-2 flex items-center gap-1.5">
                <Check size={14} aria-hidden="true" /> Checks ({packet.checks.length})
              </h3>
              <ul className="list-disc list-inside text-xs text-slate-400 space-y-1">
                {packet.checks.map((check, idx) => (
                  <li key={idx}>{check}</li>
                ))}
              </ul>
            </div>
          )}

          {/* Commands Run */}
          {packet.commands && packet.commands.length > 0 && (
            <div className="p-4 border-b border-slate-800">
              <h3 className="text-xs font-semibold uppercase text-slate-400 mb-2 flex items-center gap-1.5">
                <Terminal size={14} aria-hidden="true" /> Commands Evaluated ({packet.commands.length})
              </h3>
              <div className="space-y-2">
                {packet.commands.map((cmd, idx) => (
                  <div key={idx} className="p-2 bg-slate-900 rounded border border-slate-800 text-xs">
                    <div className="flex items-center justify-between">
                      <span className="font-mono text-slate-200 truncate">{cmd.argv.join(' ')}</span>
                      <span
                        className={`text-[10px] px-1.5 py-0.5 rounded font-bold uppercase ${
                          cmd.judge === 'allow'
                            ? 'bg-green-500/20 text-green-400'
                            : cmd.judge === 'deny'
                            ? 'bg-rose-500/20 text-rose-400'
                            : 'bg-amber-500/20 text-amber-400'
                        }`}
                      >
                        {cmd.judge}
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Risks */}
          {packet.risks && packet.risks.length > 0 && (
            <div className="p-4">
              <h3 className="text-xs font-semibold uppercase text-amber-400 mb-2 flex items-center gap-1.5">
                <AlertTriangle size={14} aria-hidden="true" /> Residual Risks ({packet.risks.length})
              </h3>
              <ul className="list-disc list-inside text-xs text-slate-400 space-y-1">
                {packet.risks.map((risk, idx) => (
                  <li key={idx}>{risk}</li>
                ))}
              </ul>
            </div>
          )}
        </div>

        {/* Right Pane: Monaco DiffEditor */}
        <div className="flex-1 min-h-0 relative bg-slate-900">
          {currentDiff ? (
            <DiffEditor
              original={currentDiff.original}
              modified={currentDiff.proposed}
              language={guessLanguage(currentDiff.path)}
              theme="vs-dark"
              options={{ readOnly: true, renderSideBySide: true, minimap: { enabled: false } }}
            />
          ) : (
            <div className="h-full flex flex-col items-center justify-center gap-1 text-slate-500 text-sm px-6 text-center">
              {packet.diffs.length === 0 ? (
                <>
                  <span className="font-medium text-slate-400">No file changes proposed</span>
                  <span className="text-xs">
                    This run produced no diffs to review. Check the verification panel for
                    whether anything was actually tested.
                  </span>
                </>
              ) : (
                'Select a diff file to review'
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
