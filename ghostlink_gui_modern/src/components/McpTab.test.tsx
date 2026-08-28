import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { McpTab } from './McpTab';
import { useAppStore } from '../store';

function createMockApi() {
  return {
    listMcpServers: vi.fn().mockResolvedValue({
      servers: [],
    }),
    toggleMcpServer: vi.fn().mockResolvedValue({ success: true, servers: [] }),
    deleteMcpServer: vi.fn().mockResolvedValue({ success: true, servers: [] }),
    createMcpServer: vi.fn().mockResolvedValue({ success: true, servers: [] }),
    updateMcpServer: vi.fn().mockResolvedValue({ success: true, servers: [] }),
  };
}

describe('McpTab', () => {
  beforeEach(() => {
    useAppStore.setState({
      mcpServers: [],
      setMcpServers: (servers) => useAppStore.setState({ mcpServers: servers }),
      addToast: vi.fn(),
    } as any);
  });

  it('renders empty state with CTA button that opens the modal', async () => {
    const api = createMockApi();
    render(<McpTab api={api} />);

    await waitFor(() => {
      expect(screen.getByText('No MCP servers configured')).toBeInTheDocument();
    });

    const addButtons = screen.getAllByRole('button', { name: /Add Server/i });
    expect(addButtons.length).toBeGreaterThan(0);

    fireEvent.click(addButtons[0]);

    await waitFor(() => {
      expect(screen.getByRole('dialog', { name: 'Add MCP Server' })).toBeInTheDocument();
    });
  });

  it('traps focus inside the dialog when Tab key is pressed', async () => {
    const api = createMockApi();
    render(<McpTab api={api} />);

    await waitFor(() => {
      expect(screen.getByText('No MCP servers configured')).toBeInTheDocument();
    });

    const addButton = screen.getAllByRole('button', { name: /Add Server/i })[0];
    fireEvent.click(addButton);

    await waitFor(() => {
      expect(screen.getByRole('dialog', { name: 'Add MCP Server' })).toBeInTheDocument();
    });

    const closeButton = screen.getByRole('button', { name: 'Close' });
    const cancelButton = screen.getByRole('button', { name: 'Cancel' });

    cancelButton.focus();
    expect(document.activeElement).toBe(cancelButton);

    // Tab from last focusable item (Save button) should cycle back to first (Close button)
    const saveButton = screen.getByRole('button', { name: 'Save' });
    saveButton.focus();
    expect(document.activeElement).toBe(saveButton);

    fireEvent.keyDown(document.activeElement!, { key: 'Tab', shiftKey: false });
    expect(document.activeElement).toBe(closeButton);

    // Shift+Tab from first focusable item (Close button) should cycle back to last (Save button)
    fireEvent.keyDown(document.activeElement!, { key: 'Tab', shiftKey: true });
    expect(document.activeElement).toBe(saveButton);
  });

  it('restores focus to trigger button when modal is closed', async () => {
    const api = createMockApi();
    render(<McpTab api={api} />);

    await waitFor(() => {
      expect(screen.getByText('No MCP servers configured')).toBeInTheDocument();
    });

    const addButton = screen.getAllByRole('button', { name: /Add Server/i })[0];
    fireEvent.click(addButton);

    await waitFor(() => {
      expect(screen.getByRole('dialog', { name: 'Add MCP Server' })).toBeInTheDocument();
    });

    const closeButton = screen.getByRole('button', { name: 'Close' });
    fireEvent.click(closeButton);

    await waitFor(() => {
      expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
      expect(document.activeElement).toBe(addButton);
    });
  });
});
