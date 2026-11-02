import React, { useState, useEffect } from "react";
import { Play, Square, Shield, Clock, Layers, KeyRound, Loader2, Copy, Check } from "lucide-react";
import { Task, ReviewPacket, TaskEvent, GhostlinkAPI } from "../api";
import { useAppStore } from "../store";
import { ReviewPane } from "./ReviewPane";

interface TaskViewProps {
  task: Task;
  api: GhostlinkAPI;
  onRefresh?: () => void;
}

export const TaskView: React.FC<TaskViewProps> = ({ task, api, onRefresh }) => {

  const [review, setReview] = useState<ReviewPacket | null>(null);
  const [spawning, setSpawning] = useState<boolean>(false);
  const [cancelling, setCancelling] = useState<boolean>(false);
  const [brief, setBrief] = useState<string>("");
  const [copiedEventIdx, setCopiedEventIdx] = useState<number | null>(null);

  const taskEvents = useAppStore((state) => state.taskEvents);
  const addTaskEvent = useAppStore((state) => state.addTaskEvent);
  const addToast = useAppStore((state) => state.addToast);

  const apiKey = api.getApiKey?.() || "";

  // Fetch review packet if task is in needs_review or blocked or accepted/rejected
  useEffect(() => {
    let active = true;
    if (["needs_review", "accepted", "rejected", "blocked"].includes(task.status)) {
      api
        .getTaskReview(task.id)
        .then((rev) => {
          if (active) setReview(rev);
        })
        .catch(() => {
          if (active) setReview(null);
        });
    } else {
      setReview(null);
    }
    return () => {
      active = false;
    };
  }, [task.id, task.status, api]);

  const [sseConnected, setSseConnected] = useState<boolean>(false);

  // Polling fallback until SSE connects
  useEffect(() => {
    let interval: any = null;
    if (!sseConnected && task.status === "running") {
      interval = setInterval(() => {
        if (onRefresh) onRefresh();
      }, 3000);
    }
    return () => {
      if (interval) clearInterval(interval);
    };
  }, [sseConnected, task.status, onRefresh]);

  // Connect to SSE event stream
  useEffect(() => {
    if (!apiKey) return;

    const baseUrl = api.getApiBaseUrl ? api.getApiBaseUrl() : "";
    const sseUrl = `${baseUrl}/api/tasks/${task.id}/events?access_token=${encodeURIComponent(apiKey)}`;
    const es = new EventSource(sseUrl);

    es.onopen = () => {
      setSseConnected(true);
    };

    es.onerror = () => {
      setSseConnected(false);
    };

    // Heartbeat. The backend sends a named `connection_status` event every 30s,
    // which is the only way to notice a silently-dropped connection: `onerror`
    // fires only when the socket actually breaks, and a middlebox that stops
    // forwarding never breaks it. Track the last heartbeat and let the polling
    // fallback take over if it goes stale.
    let lastBeat = Date.now();
    // Guarded because a minimal EventSource stand-in (as used in tests) may
    // only implement the `on*` properties, not the full EventTarget API.
    if (typeof es.addEventListener === 'function') {
      es.addEventListener('connection_status', () => {
        lastBeat = Date.now();
        setSseConnected(true);
      });
    }
    const beatWatch = window.setInterval(() => {
      // 90s with no heartbeat while the task is still running means the stream
      // is dead even though `onerror` never fired.
      if (task.status === 'running' && Date.now() - lastBeat > 90_000) {
        setSseConnected(false);
      }
    }, 15_000);

    es.onmessage = (e) => {
      try {
        const ev: TaskEvent = JSON.parse(e.data);
        addTaskEvent(ev);
        // `placement_decided` and `auto_fix_triggered` change what the header
        // shows (assigned node, "auto-fixing"), so they refresh too.
        if (
          ["needs_review", "accepted", "rejected", "cancelled", "blocked", "review_ready", "error", "placement_decided", "auto_fix_triggered"].includes(
            ev.kind
          ) &&
          onRefresh
        ) {
          onRefresh();
        }
      } catch {
        /* parse error */
      }
    };
    return () => {
      es.close();
      window.clearInterval(beatWatch);
    };
  }, [task.id, apiKey, api, addTaskEvent, onRefresh, task.status]);

  const handleSpawn = async () => {
    setSpawning(true);
    try {
      await api.spawnTask(task.id, { brief: brief.trim() || undefined });
      addToast({
        message: `Implementer run started for task #${task.id.slice(0, 8)}`,
        type: "info",
      });
      if (onRefresh) onRefresh();
    } catch (err: any) {
      addToast({
        message: err.response?.data?.error || err.message || "Failed to spawn task",
        type: "error",
      });
    } finally {
      setSpawning(false);
    }
  };

  const handleCancel = async () => {
    setCancelling(true);
    try {
      await api.cancelTask(task.id);
      addToast({
        message: `Task #${task.id.slice(0, 8)} was cancelled.`,
        type: "info",
      });
      if (onRefresh) onRefresh();
    } catch (err: any) {
      addToast({
        message: err.response?.data?.error || err.message || "Failed to cancel task",
        type: "error",
      });
    } finally {
      setCancelling(false);
    }
  };

  const relevantEvents = taskEvents.filter((e: TaskEvent) => e.task_id === task.id);

  return (
    <div className="flex flex-col h-full bg-slate-950 p-6 space-y-6 overflow-y-auto">
      {!apiKey && (
        <div className="bg-amber-500/10 border border-amber-500/20 rounded-xl p-4 flex items-center gap-3 text-xs text-amber-400">
          <KeyRound size={16} className="shrink-0" aria-hidden="true" />
          <span>API key required to stream live task events. Paste your API key in the Security tab.</span>
        </div>
      )}

      {/* Header & Meta */}
      <div className="bg-slate-900 border border-slate-800 rounded-xl p-6 flex flex-wrap items-center justify-between gap-4">
        <div>
          <div className="flex items-center gap-3">
            <span
              className={`px-2.5 py-1 rounded-full text-xs font-bold uppercase tracking-wider ${
                task.status === "needs_review"
                  ? "bg-amber-500/20 text-amber-400 border border-amber-500/30"
                  : task.status === "running"
                  ? "bg-indigo-500/20 text-indigo-400 border border-indigo-500/30 animate-pulse"
                  : task.status === "accepted"
                  ? "bg-green-500/20 text-green-400 border border-green-500/30"
                  : task.status === "rejected"
                  ? "bg-rose-500/20 text-rose-400 border border-rose-500/30"
                  : "bg-slate-800 text-slate-400 border border-slate-700"
              }`}
            >
              {task.status}
            </span>
            <h2 className="text-xl font-bold text-slate-100">{task.goal}</h2>
          </div>
          {task.acceptance_criteria && (
            <p className="text-sm text-slate-400 mt-2">
              <span className="font-semibold text-slate-300">Acceptance Criteria:</span> {task.acceptance_criteria}
            </p>
          )}
          <div className="flex items-center gap-4 text-xs text-slate-500 mt-3">
            <span className="flex items-center gap-1">
              <Clock size={12} aria-hidden="true" /> Max Steps: {task.budget.max_steps}
            </span>
            <span className="flex items-center gap-1">
              <Layers size={12} aria-hidden="true" /> Max Minutes: {task.budget.max_minutes}
            </span>
          </div>
        </div>

        {/* Action Controls */}
        <div className="flex items-center gap-3">
          {task.status !== "running" && task.status !== "needs_review" && (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                handleSpawn();
              }}
              className="flex items-center gap-2"
            >
              <input
                id="custom-task-brief"
                type="text"
                placeholder="Optional custom brief..."
                aria-label="Custom task brief"
                value={brief}
                disabled={spawning}
                onChange={(e) => setBrief(e.target.value)}
                className="px-3 py-1.5 bg-slate-950 border border-slate-700 rounded-lg text-xs text-slate-200 focus:outline-none focus:ring-2 focus:ring-indigo-500 disabled:opacity-50"
              />
              <button
                type="submit"
                disabled={spawning}
                aria-busy={spawning}
                aria-label={spawning ? "Spawning implementer run..." : "Spawn Implementer Run"}
                title={spawning ? "Spawning implementer run..." : "Spawn an implementer agent run for this task"}
                className="flex items-center gap-1.5 px-4 py-2 rounded-lg text-xs font-bold bg-indigo-600 hover:bg-indigo-500 text-white transition disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-indigo-500 focus-visible:outline-none"
              >
                {spawning ? <Loader2 size={14} className="animate-spin" aria-hidden="true" /> : <Play size={14} aria-hidden="true" />}
                {spawning ? 'Spawning...' : 'Spawn Run'}
              </button>
            </form>
          )}
          {task.status === "running" && (
            <button
              type="button"
              onClick={handleCancel}
              disabled={cancelling}
              aria-busy={cancelling}
              aria-label={cancelling ? "Cancelling task..." : "Cancel Task"}
              title={cancelling ? "Cancelling task..." : "Cancel the active task run"}
              className="flex items-center gap-1.5 px-4 py-2 rounded-lg text-xs font-bold bg-rose-700 hover:bg-rose-600 text-white transition disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-rose-500 focus-visible:outline-none"
            >
              {cancelling ? <Loader2 size={14} className="animate-spin" aria-hidden="true" /> : <Square size={14} aria-hidden="true" />}
              {cancelling ? 'Cancelling...' : 'Cancel'}
            </button>
          )}
        </div>
      </div>

      {/* Embedded ReviewPane when needs_review or review exists */}
      {review && (
        <div className="h-[600px]">
          {task.parent_id && (
            <div className="mt-3 flex items-center gap-2 text-xs text-slate-400 bg-slate-900/50 p-2 rounded border border-slate-800">
              <Layers className="w-3.5 h-3.5 text-indigo-400" />
              <span>Child task of parent: <code className="text-indigo-300">{task.parent_id}</code></span>
            </div>
          )}

          <ReviewPane packet={review} api={api} onDecided={onRefresh} />
        </div>
      )}

      {/* Live Event Stream Timeline */}
      <div className="bg-slate-900 border border-slate-800 rounded-xl p-6">
        <h3 className="text-sm font-bold uppercase tracking-wider text-slate-400 mb-4 flex items-center gap-2">
          <Shield size={16} aria-hidden="true" /> Live Agent Events Stream ({relevantEvents.length})
        </h3>
        {relevantEvents.length === 0 ? (
          <p className="text-xs text-slate-500 italic">No events emitted yet. Spawn a run to see live events.</p>
        ) : (
          <div className="space-y-3 font-mono text-xs">
            {relevantEvents.map((ev: TaskEvent, idx: number) => {
              const isCopied = copiedEventIdx === idx;
              return (
                <div key={idx} className="p-3 bg-slate-950 rounded-lg border border-slate-800 flex items-start gap-3 group">
                  <span className="text-[10px] px-2 py-0.5 rounded bg-indigo-500/20 text-indigo-400 font-bold uppercase shrink-0">
                    {ev.kind}
                  </span>
                  <span className="text-slate-500 text-[10px] shrink-0">
                    {new Date(ev.ts).toLocaleTimeString()}
                  </span>
                  <div className="flex-1 text-slate-300 overflow-x-auto">
                    <pre className="whitespace-pre-wrap">{JSON.stringify(ev.payload, null, 2)}</pre>
                  </div>
                  <button
                    type="button"
                    onClick={async () => {
                      const payloadStr = JSON.stringify(ev.payload, null, 2);
                      await navigator.clipboard.writeText(payloadStr);
                      setCopiedEventIdx(idx);
                      addToast({
                        message: `Copied event #${idx + 1} payload to clipboard`,
                        type: 'info',
                      });
                      setTimeout(() => {
                        setCopiedEventIdx((curr) => (curr === idx ? null : curr));
                      }, 2000);
                    }}
                    aria-label={isCopied ? "Copied event payload to clipboard" : `Copy event #${idx + 1} payload`}
                    title={isCopied ? "Copied payload to clipboard" : "Copy event payload (JSON)"}
                    className="p-1 rounded text-slate-500 hover:text-slate-300 hover:bg-slate-800/80 transition opacity-0 group-hover:opacity-100 focus:opacity-100 focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-indigo-500 focus-visible:outline-none shrink-0"
                  >
                    {isCopied ? (
                      <Check size={12} className="text-emerald-400" aria-hidden="true" />
                    ) : (
                      <Copy size={12} aria-hidden="true" />
                    )}
                  </button>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
};
