import React, { Component, ErrorInfo, ReactNode } from 'react';
import { AlertTriangle, RefreshCw, WifiOff } from 'lucide-react';

interface ErrorBoundaryProps {
  children: ReactNode;
  fallback?: ReactNode;
  onError?: (error: Error, errorInfo: ErrorInfo) => void;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
  errorInfo: ErrorInfo | null;
}

export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null, errorInfo: null };
  }

  static getDerivedStateFromError(error: Error): Partial<ErrorBoundaryState> {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo): void {
    this.setState({ errorInfo });
    console.error('ErrorBoundary caught error:', error, errorInfo);
    this.props.onError?.(error, errorInfo);
  }

  handleRetry = (): void => {
    this.setState({ hasError: false, error: null, errorInfo: null });
  };

  render(): ReactNode {
    if (this.state.hasError) {
      if (this.props.fallback) {
        return this.props.fallback;
      }

      return (
        <div className="flex flex-col items-center justify-center h-full p-8 bg-slate-950">
          <div className="text-center max-w-md">
            <div className="mb-6 p-4 bg-red-500/10 border border-red-500/20 rounded-2xl">
              <AlertTriangle className="mx-auto mb-3 h-12 w-12 text-red-500" />
            </div>
            <h2 className="text-xl font-bold text-white mb-2">Something went wrong</h2>
            <p className="text-slate-400 mb-6">
              {this.state.error?.message || 'An unexpected error occurred'}
            </p>
            {this.state.errorInfo && (
              <details className="text-left mb-4 p-3 bg-slate-900 border border-slate-700 rounded-lg text-xs">
                <summary className="cursor-pointer text-slate-500 mb-2">Error Details</summary>
                <pre className="text-red-400 overflow-auto max-h-48">
                  {this.state.errorInfo.componentStack}
                </pre>
              </details>
            )}
            <button
              onClick={this.handleRetry}
              className="inline-flex items-center gap-2 px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-xl text-sm font-medium transition"
            >
              <RefreshCw size={16} /> Try Again
            </button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}

interface ConnectionErrorBoundaryProps {
  children: ReactNode;
  isOnline: boolean;
}

export const ConnectionErrorBoundary: React.FC<ConnectionErrorBoundaryProps> = ({ 
  children, 
  isOnline 
}) => {
  if (!isOnline) {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 bg-slate-950">
        <div className="text-center max-w-md">
          <div className="mb-6 p-4 bg-amber-500/10 border border-amber-500/20 rounded-2xl">
            <WifiOff className="mx-auto mb-3 h-12 w-12 text-amber-500" />
          </div>
          <h2 className="text-xl font-bold text-white mb-2">Connection Lost</h2>
          <p className="text-slate-400 mb-6">
            You appear to be offline. Please check your connection and try again.
          </p>
          <button
            onClick={() => window.location.reload()}
            className="inline-flex items-center gap-2 px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-xl text-sm font-medium transition"
          >
            <RefreshCw size={16} /> Reconnect
          </button>
        </div>
      </div>
    );
  }

  return <>{children}</>;
};

interface OfflineBannerProps {
  isOnline: boolean;
}

export const OfflineBanner: React.FC<OfflineBannerProps> = ({ isOnline }) => {
  if (isOnline) return null;

  return (
    <div className="fixed bottom-4 right-4 z-50 animate-slide-up">
      <div className="flex items-center gap-3 px-4 py-3 bg-slate-900 border border-amber-500/30 rounded-xl shadow-lg">
        <div className="p-2 bg-amber-500/10 rounded-lg">
          <WifiOff className="h-5 w-5 text-amber-500" />
        </div>
        <div>
          <p className="text-sm font-medium text-white">Connection Lost</p>
          <p className="text-xs text-slate-400">Attempting to reconnect...</p>
        </div>
      </div>
    </div>
  );
};

export function useOnlineStatus(): boolean {
  const [isOnline, setIsOnline] = React.useState(
    typeof navigator !== 'undefined' ? navigator.onLine : true
  );

  React.useEffect(() => {
    const handleOnline = () => setIsOnline(true);
    const handleOffline = () => setIsOnline(false);

    window.addEventListener('online', handleOnline);
    window.addEventListener('offline', handleOffline);

    return () => {
      window.removeEventListener('online', handleOnline);
      window.removeEventListener('offline', handleOffline);
    };
  }, []);

  return isOnline;
}