import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { ProjectsTab } from './ProjectsTab';
import { useAppStore } from '../store';

vi.mock('@monaco-editor/react', () => {
  const DiffEditor = ({ original, modified }: any) => {
    return (
      <div data-testid="mock-diff-editor">
        <div>{original}</div>
        <div>{modified}</div>
      </div>
    );
  };
  return {
    __esModule: true,
    default: () => <div />,
    DiffEditor,
  };
});

describe('ProjectsTab', () => {
  let mockApi: any;

  beforeEach(() => {
    useAppStore.setState({
      projects: [],
      activeProject: null,
      tasks: [],
      activeTask: null,
      reviewPacket: null,
      taskEvents: [],
      unreadNeedsReviewCount: 0,
    });

    mockApi = {
      listProjects: vi.fn().mockResolvedValue([
        {
          id: 'proj_1',
          name: 'Core System',
          kind: 'code',
          root_path: '/workspace/core',
          allowed_tools: ['file_operations'],
          created_at: '2026-08-10T10:00:00Z',
        },
      ]),
      listProjectTasks: vi.fn().mockResolvedValue([
        {
          id: 'task_1',
          project_id: 'proj_1',
          goal: 'Implement bugfix',
          status: 'needs_review',
          budget: { max_steps: 24, max_minutes: 15 },
          created_at: '2026-08-10T11:00:00Z',
          updated_at: '2026-08-10T11:05:00Z',
        },
      ]),
      createProject: vi.fn(),
      createTask: vi.fn(),
      getTaskReview: vi.fn().mockResolvedValue(null),
    };
  });

  it('renders project list and fetches project tasks on mount', async () => {
    render(<ProjectsTab api={mockApi} />);

    await waitFor(() => {
      expect(mockApi.listProjects).toHaveBeenCalled();
      expect(screen.getByText('Core System')).toBeInTheDocument();
    });

    await waitFor(() => {
      expect(mockApi.listProjectTasks).toHaveBeenCalledWith('proj_1');
      expect(screen.getByText('Implement bugfix')).toBeInTheDocument();
    });
  });

  it('shows badge when task enters needs_review status', async () => {
    render(<ProjectsTab api={mockApi} />);

    await waitFor(() => {
      expect(screen.getByText('needs_review')).toBeInTheDocument();
      expect(useAppStore.getState().unreadNeedsReviewCount).toBe(1);
    });
  });

  it('shows accessible loading state when creating a project', async () => {
    let resolveCreate: any;
    const createPromise = new Promise((resolve) => {
      resolveCreate = resolve;
    });
    mockApi.createProject.mockImplementation(() => createPromise);

    render(<ProjectsTab api={mockApi} />);

    await waitFor(() => {
      expect(screen.getByText('Core System')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole('button', { name: 'New Project' }));

    const dialog = screen.getByRole('dialog', { name: 'Create New Project' });
    expect(dialog).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText('Project Name'), { target: { value: 'New Test Project' } });
    fireEvent.change(screen.getByLabelText(/Root Path/), { target: { value: '/workspace/test' } });

    const submitBtn = screen.getByRole('button', { name: 'Create project' });
    fireEvent.click(submitBtn);

    expect(submitBtn).toHaveAttribute('aria-busy', 'true');
    expect(submitBtn).toHaveAttribute('aria-label', 'Creating project...');
    expect(submitBtn).toBeDisabled();

    resolveCreate({
      id: 'proj_2',
      name: 'New Test Project',
      kind: 'code',
      root_path: '/workspace/test',
      created_at: '2026-08-10T12:00:00Z',
    });

    await waitFor(() => {
      expect(mockApi.createProject).toHaveBeenCalledWith({
        name: 'New Test Project',
        root_path: '/workspace/test',
        kind: 'code',
      });
    });
  });

  it('dismisses creation modal on Escape key press and supports accessible cancel controls', async () => {
    render(<ProjectsTab api={mockApi} />);

    await waitFor(() => {
      expect(screen.getByText('Core System')).toBeInTheDocument();
    });

    // Open project creation modal
    fireEvent.click(screen.getByRole('button', { name: 'New Project' }));
    expect(screen.getByRole('dialog', { name: 'Create New Project' })).toBeInTheDocument();

    // Verify cancel button accessibility attributes
    const cancelProjBtn = screen.getByRole('button', { name: 'Cancel project creation' });
    expect(cancelProjBtn).toBeInTheDocument();
    expect(cancelProjBtn).toHaveAttribute('title', 'Cancel project creation');

    // Press Escape to dismiss modal
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByRole('dialog', { name: 'Create New Project' })).not.toBeInTheDocument();

    // Open task creation modal
    fireEvent.click(screen.getByRole('button', { name: 'New Task' }));
    expect(screen.getByRole('dialog', { name: /Create Task for/ })).toBeInTheDocument();

    const cancelTaskBtn = screen.getByRole('button', { name: 'Cancel task creation' });
    expect(cancelTaskBtn).toBeInTheDocument();
    expect(cancelTaskBtn).toHaveAttribute('title', 'Cancel task creation');

    // Press Escape to dismiss task creation modal
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByRole('dialog', { name: /Create Task for/ })).not.toBeInTheDocument();
  });
});
