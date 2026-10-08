import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import App, { SplashScreen } from './App';
import { useAppStore } from './store';
import fs from 'fs';
import path from 'path';

vi.mock('./api', () => ({
  GhostlinkAPI: vi.fn().mockImplementation(() => ({
    getModels: vi.fn().mockReturnValue(new Promise(() => {})), // pending
    getHealth: vi.fn().mockReturnValue(new Promise(() => {})), // pending
    getMetrics: vi.fn().mockReturnValue(new Promise(() => {})), // pending
    getSessions: vi.fn().mockReturnValue(new Promise(() => {})), // pending
    listSessions: vi.fn().mockReturnValue(new Promise(() => {})), // pending
    getWorkers: vi.fn().mockReturnValue(new Promise(() => {})), // pending
    listMcpServers: vi.fn().mockReturnValue(new Promise(() => {})), // pending
    getSettings: vi.fn().mockReturnValue(new Promise(() => {})), // pending
  })),
}));

describe('App', () => {
  let mockSetApiBase: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    mockSetApiBase = vi.fn();
    useAppStore.setState({
      apiBase: 'http://localhost:8003',
      backendOnline: false,
      currentModel: 'none',
      uptime: 0,
      models: [],
      metrics: null,
      sessions: [],
      workers: [],
      selectedModel: null,
      activeTab: 0,
      setApiBase: mockSetApiBase,
      setBackendOnline: vi.fn(),
      setCurrentModel: vi.fn(),
      setUptime: vi.fn(),
      setModels: vi.fn(),
      setMetrics: vi.fn(),
      setSessions: vi.fn(),
      setWorkers: vi.fn(),
      setSelectedModel: vi.fn(),
      setActiveTab: vi.fn(),
    });
  });

  it('initializes API base on app load', () => {
    render(<App />);
    expect(mockSetApiBase).toHaveBeenCalledWith('http://127.0.0.1:8000');
  });

  it('renders sidebar and nav tabs while health check is pending', () => {
    render(<App />);
    expect(screen.getByText('Ghostlink')).toBeInTheDocument();
    expect(screen.getByText('Chat')).toBeInTheDocument();
    expect(screen.getByText('Models')).toBeInTheDocument();
    expect(screen.getByText('Editor')).toBeInTheDocument();
    expect(screen.getByText('Connecting to Ghostlink backend...')).toBeInTheDocument();
  });

  it('shows all navigation tabs', () => {
    render(<App />);
    expect(screen.getByText('Chat')).toBeInTheDocument();
    expect(screen.getByText('Models')).toBeInTheDocument();
    expect(screen.getByText('Metrics')).toBeInTheDocument();
    expect(screen.getByText('Sessions')).toBeInTheDocument();
    expect(screen.getByText('Workers')).toBeInTheDocument();
    expect(screen.getByText('Security')).toBeInTheDocument();
    expect(screen.getByText('Settings')).toBeInTheDocument();
  });

  it('shows New Chat button', () => {
    render(<App />);
    expect(screen.getAllByText('New Chat')[0]).toBeInTheDocument();
  });

  it('renders SplashScreen with proper accessibility attributes', () => {
    const { getByRole, getByLabelText } = render(<SplashScreen />);
    const spinner = getByLabelText('Loading Ghostlink Studio');
    expect(spinner).toBeInTheDocument();

    const statusContainer = getByRole('status');
    expect(statusContainer).toBeInTheDocument();
    expect(statusContainer).toHaveAttribute('aria-live', 'polite');
  });

  it('renders SplashScreen skip button when onDismiss is provided and triggers dismissal', () => {
    const handleDismiss = vi.fn();
    render(<SplashScreen currentStep={2} onDismiss={handleDismiss} />);
    const skipBtn = screen.getByRole('button', { name: /Skip loading screen and open Ghostlink Studio/i });
    expect(skipBtn).toBeInTheDocument();
    skipBtn.click();
    expect(handleDismiss).toHaveBeenCalledTimes(1);
  });

  it('verifies src/main.tsx does not statically import monacoSetup or EditorTab', () => {
    const mainPath = path.resolve(__dirname, 'main.tsx');
    const mainContent = fs.readFileSync(mainPath, 'utf-8');
    expect(mainContent).not.toContain('monacoSetup');
    expect(mainContent).not.toContain('EditorTab');
  });
});
