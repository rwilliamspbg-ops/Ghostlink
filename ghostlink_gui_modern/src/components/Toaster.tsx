import React, { useEffect, useRef, useCallback } from 'react';
import { CheckCircle2, XCircle, Info, X } from 'lucide-react';
import { useAppStore, Toast } from '../store';

const ICONS = { success: CheckCircle2, error: XCircle, info: Info } as const;
const STYLES = {
  success: 'bg-slate-900 border-green-900/50 text-green-200',
  error: 'bg-red-950 border-red-900 text-red-200',
  info: 'bg-slate-900 border-slate-800 text-slate-200',
} as const;

const AUTO_DISMISS_MS = 5000;

const ToastItem: React.FC<Toast> = ({ id, type, message }) => {
  const removeToast = useAppStore((s) => s.removeToast);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const remainingTimeRef = useRef<number>(AUTO_DISMISS_MS);
  const startTimeRef = useRef<number>(Date.now());

  const startTimer = useCallback(() => {
    startTimeRef.current = Date.now();
    timerRef.current = setTimeout(() => {
      removeToast(id);
    }, remainingTimeRef.current);
  }, [id, removeToast]);

  const pauseTimer = useCallback(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
      const elapsed = Date.now() - startTimeRef.current;
      remainingTimeRef.current = Math.max(0, remainingTimeRef.current - elapsed);
    }
  }, []);

  useEffect(() => {
    startTimer();
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [startTimer]);

  const Icon = ICONS[type];

  return (
    <div
      role="alert"
      onMouseEnter={pauseTimer}
      onMouseLeave={startTimer}
      onFocus={pauseTimer}
      onBlur={startTimer}
      className={`flex items-start gap-3 px-4 py-3 rounded-2xl border shadow-2xl animate-in fade-in slide-in-from-right-4 duration-200 ${STYLES[type]}`}
    >
      <Icon size={18} className="flex-shrink-0 mt-0.5" aria-hidden="true" />
      <p className="text-sm flex-1 min-w-0 break-words">{message}</p>
      <button
        onClick={() => removeToast(id)}
        className="text-current opacity-60 hover:opacity-100 transition shrink-0 rounded focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
        aria-label="Dismiss notification"
        title="Dismiss notification"
      >
        <X size={14} aria-hidden="true" />
      </button>
    </div>
  );
};

export const Toaster: React.FC = () => {
  const toasts = useAppStore((s) => s.toasts);
  if (toasts.length === 0) return null;

  return (
    <div
      className="fixed bottom-24 right-4 z-[90] flex flex-col gap-2 w-full max-w-sm px-4 sm:px-0"
      aria-live="polite"
      aria-atomic="true"
      aria-label="Notifications"
    >
      {toasts.map((t) => (
        <ToastItem key={t.id} {...t} />
      ))}
    </div>
  );
};
