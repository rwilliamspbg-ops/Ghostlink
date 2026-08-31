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

  it('cycles through commands with Tab and Shift+Tab key navigation', () => {
    render(<CommandPalette />);

    act(() => {
      window.dispatchEvent(new CustomEvent('open-command-palette'));
    });

    const searchInput = screen.getByRole('combobox', { name: 'Search commands' });
    const options = screen.getAllByRole('option');
    expect(options[0]).toHaveAttribute('aria-selected', 'true');

    // Tab moves highlight to next option
    fireEvent.keyDown(searchInput, { key: 'Tab' });
    expect(options[1]).toHaveAttribute('aria-selected', 'true');

    // Shift+Tab cycles back to previous option
    fireEvent.keyDown(searchInput, { key: 'Tab', shiftKey: true });
    expect(options[0]).toHaveAttribute('aria-selected', 'true');

    // Shift+Tab at first option wraps around to last option
    fireEvent.keyDown(searchInput, { key: 'Tab', shiftKey: true });
    expect(options[options.length - 1]).toHaveAttribute('aria-selected', 'true');
  });

  it('contains Phase 1-5 primary action commands', () => {
    render(<CommandPalette />);

    act(() => {
      window.dispatchEvent(new CustomEvent('open-command-palette'));
    });

    const expectedCommandIds = [
      'retry-health',
      'set-api-key',
      'open-health',
      'download-models',
      'load-model',
      'unload-model',
      'new-chat',
      'search-threads',
      'prompt-presets',
      'discover-peers',
      'use-other-machines',
      'enable-calculator',
      'index-workspace',
      'toggle-workspace-context',
    ];

    const options = screen.getAllByRole('option');
    const optionIds = options.map((opt) => opt.id.replace('command-', ''));

    for (const cmdId of expectedCommandIds) {
      expect(optionIds).toContain(cmdId);
    }
  });

  it('triggers custom events and tab navigation on command selection', () => {
    const setActiveTabMock = vi.fn();
    useAppStore.setState({ setActiveTab: setActiveTabMock });

    render(<CommandPalette />);

    act(() => {
      window.dispatchEvent(new CustomEvent('open-command-palette'));
    });

    const discoverPeersOption = screen.getByText('Discover LAN peers');
    const customEventSpy = vi.fn();
    window.addEventListener('discover-lan-peers', customEventSpy);

    fireEvent.click(discoverPeersOption);

    expect(setActiveTabMock).toHaveBeenCalledWith(4);
    expect(customEventSpy).toHaveBeenCalled();
  });
});
