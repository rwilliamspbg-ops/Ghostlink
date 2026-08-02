import React from 'react';
import { Loader, type LucideIcon } from 'lucide-react';

type IconType = LucideIcon;

interface EmptyStateProps {
  icon: IconType;
  title: string;
  description?: React.ReactNode;
  /** 'card' draws the dashed bordered card seen on Sessions/Workers; 'plain' is bare centered text (Models/MCP). */
  variant?: 'card' | 'plain';
  className?: string;
}

// Single shared shape for "nothing here yet" across tabs — previously every
// tab hand-rolled a slightly different version of the same empty state.
export const EmptyState: React.FC<EmptyStateProps> = ({ icon: Icon, title, description, variant = 'plain', className = '' }) => (
  <div
    className={`flex flex-col items-center justify-center text-center py-16 text-slate-500 ${
      variant === 'card' ? 'bg-slate-900/30 border border-slate-800 border-dashed rounded-3xl' : ''
    } ${className}`}
  >
    <Icon size={48} className="mb-4 opacity-30 text-slate-700" aria-hidden="true" />
    <p className="text-base font-medium text-slate-400">{title}</p>
    {description && <p className="text-sm text-slate-500 mt-1 max-w-sm">{description}</p>}
  </div>
);

interface LoadingStateProps {
  label?: string;
  className?: string;
}

export const LoadingState: React.FC<LoadingStateProps> = ({ label = 'Loading…', className = '' }) => (
  <div role="status" aria-live="polite" className={`flex flex-col items-center justify-center py-16 text-slate-500 ${className}`}>
    <Loader size={32} className="mb-3 text-blue-500 animate-spin" aria-hidden="true" />
    <p className="text-sm">{label}</p>
  </div>
);

interface ErrorPanelProps {
  icon?: IconType;
  title?: string;
  message: string;
  className?: string;
}

export const ErrorPanel: React.FC<ErrorPanelProps> = ({ icon: Icon, title = 'Something went wrong', message, className = '' }) => (
  <div
    role="alert"
    className={`flex flex-col items-center justify-center text-center py-16 bg-slate-900/30 border border-red-800/50 border-dashed rounded-3xl ${className}`}
  >
    {Icon && <Icon size={48} className="mb-4 opacity-40 text-red-400" aria-hidden="true" />}
    <p className="text-base font-medium text-red-400">{title}</p>
    <p className="text-sm text-slate-500 opacity-80 mt-1 max-w-sm">{message}</p>
  </div>
);
