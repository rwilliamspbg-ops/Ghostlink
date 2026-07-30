import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { EditorTab } from './EditorTab';
import { useAppStore } from '../store';
import { GhostlinkAPI } from '../api';

// EditorTab renders a real Monaco `Editor`/`DiffEditor`, which needs browser
// APIs (workers, canvas text measurement) jsdom doesn't provide. Stand in
// with plain form controls that expose the same value/onChange/onMount
// contract EditorTab actually depends on.
vi.mock('@monaco-editor/react', () => {
  // EditorTab's onMount also registers an inline-completions provider via
  // the second (monaco namespace) argument real @monaco-editor/react passes.
  const fakeMonaco = {
    languages: { registerInlineCompletionsProvider: () => ({ dispose: () => {} }) },
    Range: class {},
  };
  const Editor = ({ value, onChange, onMount }: any) => {
    onMount?.({ layout: () => {}, getSelection: () => null, getModel: () => null }, fakeMonaco);
    return (
      <textarea
        data-testid="mock-editor"
        value={value}
        onChange={(e: any) => onChange?.(e.target.value)}
      />
    );
  };
  const DiffEditor = ({ original, modified, onMount }: any) => {
    onMount?.({ layout: () => {} });
    return (
      <div data-testid="mock-diff-editor">
        <pre data-testid="diff-original">{original}</pre>
        <pre data-testid="diff-modified">{modified}</pre>
      </div>
    );
  };
  return { __esModule: true, default: Editor, DiffEditor };
});

// EditorTab's onMount wires up a ResizeObserver (see the comment in
// EditorTab.tsx on why) — jsdom has no real implementation.
class MockResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

function createMockApi(): GhostlinkAPI {
  const api = new GhostlinkAPI('http://localhost:8003');
  vi.spyOn(api, 'getWorkspaceTree').mockResolvedValue({
    path: '',
    entries: [
      { name: 'README.md', path: 'README.md', is_dir: false, size: 123 },
      { name: 'src', path: 'src', is_dir: true, size: 0 },
    ],
  });
  vi.spyOn(api, 'getWorkspaceFile').mockResolvedValue({ path: 'README.md', content: '# Hello' });
  vi.spyOn(api, 'writeWorkspaceFile').mockResolvedValue({ success: true });
  vi.spyOn(api, 'sendMessage').mockResolvedValue({ success: true, data: { response: 'This file says hello.' } });
  vi.spyOn(api, 'indexWorkspace').mockResolvedValue({ status: 'skipped', reason: 'not connected' });
  return api;
}

describe('EditorTab', () => {
  beforeEach(() => {
    vi.stubGlobal('ResizeObserver', MockResizeObserver);
    useAppStore.setState({
      currentModel: 'none',
      editorOpenPath: null,
      editorContent: '',
      editorOriginalContent: '',
      editorPendingDiff: null,
      toasts: [],
    });
  });

  // Asserts the repo-aware-context auto-index here (rather than its own
  // test) because `workspaceIndexAttempted` in EditorTab.tsx is a
  // module-level guard, not component state — it only fires once for the
  // whole test file's lifetime, on whichever test mounts EditorTab first.
  it('loads and renders the root file tree, and silently kicks off workspace indexing', async () => {
    const api = createMockApi();
    render(<EditorTab api={api} />);
    await waitFor(() => {
      expect(api.getWorkspaceTree).toHaveBeenCalledWith('');
      expect(screen.getByText('README.md')).toBeInTheDocument();
      expect(screen.getByText('src')).toBeInTheDocument();
    });
    await waitFor(() => expect(api.indexWorkspace).toHaveBeenCalled());
    // "skipped" (rag not connected) must not surface a toast — that's the
    // expected outcome for most installs, not a failure worth interrupting for.
    expect(useAppStore.getState().toasts).toHaveLength(0);
  });

  it('opens a file on click', async () => {
    const api = createMockApi();
    render(<EditorTab api={api} />);
    await waitFor(() => expect(screen.getByText('README.md')).toBeInTheDocument());

    fireEvent.click(screen.getByText('README.md'));

    await waitFor(() => {
      expect(api.getWorkspaceFile).toHaveBeenCalledWith('README.md');
      expect(screen.getByTestId('mock-editor')).toHaveValue('# Hello');
    });
  });

  it('expands a directory node and lists its children', async () => {
    const api = createMockApi();
    (api.getWorkspaceTree as any).mockImplementation((path: string) => {
      if (path === '') {
        return Promise.resolve({ path: '', entries: [{ name: 'src', path: 'src', is_dir: true, size: 0 }] });
      }
      return Promise.resolve({ path, entries: [{ name: 'App.tsx', path: 'src/App.tsx', is_dir: false, size: 10 }] });
    });
    render(<EditorTab api={api} />);
    await waitFor(() => expect(screen.getByText('src')).toBeInTheDocument());

    fireEvent.click(screen.getByText('src'));

    await waitFor(() => {
      expect(api.getWorkspaceTree).toHaveBeenCalledWith('src');
      expect(screen.getByText('App.tsx')).toBeInTheDocument();
    });
  });

  it('enables Save once the buffer is edited, and writes on click', async () => {
    const api = createMockApi();
    render(<EditorTab api={api} />);
    await waitFor(() => expect(screen.getByText('README.md')).toBeInTheDocument());
    fireEvent.click(screen.getByText('README.md'));
    await waitFor(() => expect(screen.getByTestId('mock-editor')).toHaveValue('# Hello'));

    const saveButton = screen.getByRole('button', { name: 'Save' });
    expect(saveButton).toBeDisabled();

    fireEvent.change(screen.getByTestId('mock-editor'), { target: { value: '# Hello, edited' } });
    expect(saveButton).not.toBeDisabled();

    fireEvent.click(saveButton);
    await waitFor(() => {
      expect(api.writeWorkspaceFile).toHaveBeenCalledWith('README.md', '# Hello, edited');
    });
  });

  it('runs Explain and renders the reply', async () => {
    const api = createMockApi();
    render(<EditorTab api={api} />);
    await waitFor(() => expect(screen.getByText('README.md')).toBeInTheDocument());
    fireEvent.click(screen.getByText('README.md'));
    await waitFor(() => expect(screen.getByTestId('mock-editor')).toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: 'Explain' }));

    await waitFor(() => {
      expect(api.sendMessage).toHaveBeenCalled();
      expect(screen.getByText('This file says hello.')).toBeInTheDocument();
    });
    // Explain is read-only — it must never propose a diff.
    expect(screen.queryByTestId('mock-diff-editor')).not.toBeInTheDocument();
  });

  it('shows a diff preview for Fix, and Accept writes the proposed content', async () => {
    const api = createMockApi();
    (api.sendMessage as any).mockResolvedValue({
      success: true,
      data: { response: '```\n# Hello, fixed\n```' },
    });
    render(<EditorTab api={api} />);
    await waitFor(() => expect(screen.getByText('README.md')).toBeInTheDocument());
    fireEvent.click(screen.getByText('README.md'));
    await waitFor(() => expect(screen.getByTestId('mock-editor')).toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: 'Fix' }));

    await waitFor(() => {
      expect(screen.getByTestId('mock-diff-editor')).toBeInTheDocument();
      expect(screen.getByTestId('diff-modified')).toHaveTextContent('# Hello, fixed');
    });

    fireEvent.click(screen.getByRole('button', { name: 'Accept' }));

    await waitFor(() => {
      expect(api.writeWorkspaceFile).toHaveBeenCalledWith('README.md', '# Hello, fixed');
      expect(screen.queryByTestId('mock-diff-editor')).not.toBeInTheDocument();
    });
  });

  it('Reject discards the proposed diff without writing', async () => {
    const api = createMockApi();
    (api.sendMessage as any).mockResolvedValue({
      success: true,
      data: { response: '```\n# Hello, refactored\n```' },
    });
    render(<EditorTab api={api} />);
    await waitFor(() => expect(screen.getByText('README.md')).toBeInTheDocument());
    fireEvent.click(screen.getByText('README.md'));
    await waitFor(() => expect(screen.getByTestId('mock-editor')).toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: 'Refactor' }));
    await waitFor(() => expect(screen.getByTestId('mock-diff-editor')).toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: 'Reject' }));

    await waitFor(() => {
      expect(screen.queryByTestId('mock-diff-editor')).not.toBeInTheDocument();
    });
    expect(api.writeWorkspaceFile).not.toHaveBeenCalled();
  });

  it('multi-file refactor: select a file, run, and Accept writes the proposed change', async () => {
    const api = createMockApi();
    (api.sendMessage as any).mockResolvedValue({
      success: true,
      data: { response: '### FILE: README.md\n```\n# Hello, multi-refactored\n```' },
    });
    render(<EditorTab api={api} />);
    await waitFor(() => expect(screen.getByText('README.md')).toBeInTheDocument());

    fireEvent.click(screen.getByLabelText('Select README.md for multi-file refactor'));
    const refactorButton = await screen.findByRole('button', { name: /Refactor Selected/ });
    fireEvent.click(refactorButton);

    await waitFor(() => {
      const call = (api.sendMessage as any).mock.calls.at(-1)[0];
      expect(call.message).toContain('### FILE: README.md');
    });

    await waitFor(() => {
      expect(screen.getByText(/Multi-file refactor/)).toBeInTheDocument();
      expect(screen.getByTestId('diff-modified')).toHaveTextContent('# Hello, multi-refactored');
    });

    fireEvent.click(screen.getByRole('button', { name: 'Accept' }));

    await waitFor(() => {
      expect(api.writeWorkspaceFile).toHaveBeenCalledWith('README.md', '# Hello, multi-refactored');
      expect(screen.queryByText(/Multi-file refactor/)).not.toBeInTheDocument();
    });
  });

  it('toggles the ghost-text autocomplete button state', async () => {
    const api = createMockApi();
    render(<EditorTab api={api} />);
    await waitFor(() => expect(screen.getByText('README.md')).toBeInTheDocument());

    const toggle = screen.getByRole('button', { name: 'Ghost-text autocomplete' });
    expect(toggle).toHaveAttribute('aria-pressed', 'false');

    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-pressed', 'true');
  });

  it('shows an error toast when opening a file fails', async () => {
    const api = createMockApi();
    (api.getWorkspaceFile as any).mockResolvedValue({ path: 'README.md', content: '', error: 'not found' });
    render(<EditorTab api={api} />);
    await waitFor(() => expect(screen.getByText('README.md')).toBeInTheDocument());

    fireEvent.click(screen.getByText('README.md'));

    await waitFor(() => {
      expect(useAppStore.getState().toasts.some((t) => t.type === 'error' && t.message === 'not found')).toBe(true);
    });
    expect(screen.getByText('No file open')).toBeInTheDocument();
  });
});
