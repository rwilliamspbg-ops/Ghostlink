import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { TaskView } from './TaskView';
import { useAppStore } from '../store';
import { Task } from '../api';

describe('TaskView', () => {
  let mockApi: any;
  let mockEventSourceInstances: any[] = [];
  const OriginalEventSource = globalThis.EventSource;

  const sampleTask: Task = {
    id: 'task_1',
    project_id: 'proj_1',
    goal: 'Test Task Goal',
    status: 'running',
    budget: { max_steps: 10, max_minutes: 5 },
    created_at: '2026-08-10T10:00:00Z',
    updated_at: '2026-08-10T10:05:00Z',
  };

  beforeEach(() => {
    useAppStore.setState({
      taskEvents: [],
      addToast: vi.fn(),
      addTaskEvent: vi.fn(),
    });

    mockEventSourceInstances = [];
    globalThis.EventSource = vi.fn().mockImplementation((url: string) => {
      const instance = {
        url,
        close: vi.fn(),
        onmessage: null,
      };
      mockEventSourceInstances.push(instance);
      return instance;
    }) as any;

    mockApi = {
      getApiKey: vi.fn(),
      getApiBaseUrl: vi.fn().mockReturnValue("").mockReturnValue(''),
      getTaskReview: vi.fn().mockResolvedValue(null),
      spawnTask: vi.fn().mockResolvedValue({}),
      cancelTask: vi.fn().mockResolvedValue({}),
    };
  });

  afterEach(() => {
    globalThis.EventSource = OriginalEventSource;
  });

  it('renders API key required notice when getApiKey returns empty string and does not create EventSource', () => {
    mockApi.getApiKey.mockReturnValue('');
    render(<TaskView task={sampleTask} api={mockApi} />);

    expect(screen.getByText(/API key required to stream live task events/i)).toBeInTheDocument();
    expect(mockEventSourceInstances.length).toBe(0);
  });

  it('connects EventSource with encoded access_token query param when API key is set', () => {
    mockApi.getApiKey.mockReturnValue('key_secret+123');
    render(<TaskView task={sampleTask} api={mockApi} />);

    expect(screen.queryByText(/API key required to stream live task events/i)).not.toBeInTheDocument();
    expect(mockEventSourceInstances.length).toBe(1);
    expect(mockEventSourceInstances[0].url).toBe(
      '/api/tasks/task_1/events?access_token=key_secret%2B123'
    );
  });

  it('renders custom brief input with explicit aria-label and spawn button with aria attributes when task is idle', () => {
    const idleTask: Task = { ...sampleTask, status: 'created' };
    render(<TaskView task={idleTask} api={mockApi} />);

    const briefInput = screen.getByLabelText('Custom task brief');
    expect(briefInput).toBeInTheDocument();

    const spawnBtn = screen.getByRole('button', { name: 'Spawn Implementer Run' });
    expect(spawnBtn).toBeInTheDocument();
    expect(spawnBtn).toHaveAttribute('aria-busy', 'false');
    expect(spawnBtn).toHaveAttribute('title', 'Spawn an implementer agent run for this task');
  });

  it('submits form on enter press or button click, disables brief input during spawn, and calls spawnTask with brief text', async () => {
    const idleTask: Task = { ...sampleTask, status: 'created' };
    let resolveSpawn: (val: any) => void = () => {};
    const spawnPromise = new Promise((resolve) => {
      resolveSpawn = resolve;
    });
    mockApi.spawnTask.mockImplementation(() => spawnPromise);

    render(<TaskView task={idleTask} api={mockApi} />);

    const briefInput = screen.getByLabelText('Custom task brief');
    fireEvent.change(briefInput, { target: { value: 'Fix bug in auth' } });

    const spawnBtn = screen.getByRole('button', { name: 'Spawn Implementer Run' });
    fireEvent.click(spawnBtn);

    expect(mockApi.spawnTask).toHaveBeenCalledWith('task_1', { brief: 'Fix bug in auth' });
    expect(briefInput).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Spawning implementer run...' })).toBeDisabled();

    await act(async () => {
      resolveSpawn({});
    });
  });

  it('renders cancel button with proper accessibility attributes when task is running', () => {
    render(<TaskView task={sampleTask} api={mockApi} />);

    const cancelBtn = screen.getByRole('button', { name: 'Cancel Task' });
    expect(cancelBtn).toBeInTheDocument();
    expect(cancelBtn).toHaveAttribute('aria-busy', 'false');
    expect(cancelBtn).toHaveAttribute('title', 'Cancel the active task run');
  });

  it('renders copy-to-clipboard button on event items and copies payload with accessible feedback', async () => {
    const mockAddToast = vi.fn();
    useAppStore.setState({
      taskEvents: [
        {
          task_id: 'task_1',
          kind: 'step_completed',
          payload: { step: 1, action: 'read_file' },
          ts: '2026-08-10T10:01:00Z',
        },
      ],
      addToast: mockAddToast,
    });

    const writeTextMock = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, {
      clipboard: {
        writeText: writeTextMock,
      },
    });

    render(<TaskView task={sampleTask} api={mockApi} />);

    const copyBtn = screen.getByRole('button', { name: 'Copy event #1 payload' });
    expect(copyBtn).toBeInTheDocument();
    expect(copyBtn).toHaveAttribute('title', 'Copy event payload (JSON)');
    expect(copyBtn).toHaveClass('focus-visible:ring-indigo-500');

    await act(async () => {
      fireEvent.click(copyBtn);
    });

    const expectedJson = JSON.stringify({ step: 1, action: 'read_file' }, null, 2);
    expect(writeTextMock).toHaveBeenCalledWith(expectedJson);
    expect(mockAddToast).toHaveBeenCalledWith({
      message: 'Copied event #1 payload to clipboard',
      type: 'info',
    });

    expect(screen.getByRole('button', { name: 'Copied event payload to clipboard' })).toBeInTheDocument();
  });
});
