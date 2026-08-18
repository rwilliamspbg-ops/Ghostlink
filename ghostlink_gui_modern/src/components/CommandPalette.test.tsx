import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { CommandPalette } from './CommandPalette';
import { useAppStore } from '../store';

describe('CommandPalette', () => {
  beforeEach(() => {
    useAppStore.setState({
      activeTab: 0,
      chatMessages: [],
      setActiveTab: vi.fn(),
      setChatMessages: vi.fn(),
    });
  });

  it('opens command palette on custom event and supports keyboard navigation with auto-scroll', () => {
    render(<CommandPalette />);

    // Open palette via custom event
    act(() => {
      window.dispatchEvent(new CustomEvent('open-command-palette'));
    });

    const searchInput = screen.getByRole('combobox', { name: 'Search commands' });
    expect(searchInput).toBeInTheDocument();

    const scrollIntoViewMock = vi.fn();
    window.HTMLElement.prototype.scrollIntoView = scrollIntoViewMock;

    // Navigate down using ArrowDown key
    fireEvent.keyDown(searchInput, { key: 'ArrowDown' });

    expect(scrollIntoViewMock).toHaveBeenCalled();
  });
});
