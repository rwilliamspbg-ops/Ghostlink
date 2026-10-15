import React from 'react';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { describe, expect, it, beforeEach, vi, afterEach } from 'vitest';
import { Toaster } from './Toaster';
import { useAppStore } from '../store';

describe('Toaster', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    act(() => {
      useAppStore.setState({ toasts: [] });
    });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders nothing when there are no toasts', () => {
    const { container } = render(<Toaster />);
    expect(container.firstChild).toBeNull();
  });

  it('renders toast alerts with role="alert", aria-atomic="true", and aria-label on toaster container', () => {
    act(() => {
      useAppStore.getState().addToast({ type: 'success', message: 'Model downloaded successfully' });
    });

    render(<Toaster />);

    const container = screen.getByLabelText('Notifications');
    expect(container).toBeInTheDocument();
    expect(container).toHaveAttribute('aria-live', 'polite');
    expect(container).toHaveAttribute('aria-atomic', 'true');

    const alert = screen.getByRole('alert');
    expect(alert).toBeInTheDocument();
    expect(screen.getByText('Model downloaded successfully')).toBeInTheDocument();
  });

  it('renders dismiss button with aria-label and title tooltip', () => {
    act(() => {
      useAppStore.getState().addToast({ type: 'error', message: 'Connection failed' });
    });

    render(<Toaster />);

    const dismissButton = screen.getByRole('button', { name: 'Dismiss notification' });
    expect(dismissButton).toBeInTheDocument();
    expect(dismissButton).toHaveAttribute('title', 'Dismiss notification');
  });

  it('removes toast when dismiss button is clicked', () => {
    act(() => {
      useAppStore.getState().addToast({ type: 'info', message: 'System update available' });
    });

    render(<Toaster />);

    expect(screen.getByText('System update available')).toBeInTheDocument();

    const dismissButton = screen.getByRole('button', { name: 'Dismiss notification' });
    fireEvent.click(dismissButton);

    expect(screen.queryByText('System update available')).not.toBeInTheDocument();
  });

  it('pauses auto-dismiss timer on hover and resumes on mouse leave', () => {
    act(() => {
      useAppStore.getState().addToast({ type: 'info', message: 'Hover test message' });
    });

    render(<Toaster />);
    const alert = screen.getByRole('alert');

    // Fast-forward 2000ms
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(screen.getByText('Hover test message')).toBeInTheDocument();

    // Hover over the toast
    fireEvent.mouseEnter(alert);

    // Fast-forward 4000ms (exceeding original 5000ms total)
    act(() => {
      vi.advanceTimersByTime(4000);
    });
    // Message should still be visible because timer was paused
    expect(screen.getByText('Hover test message')).toBeInTheDocument();

    // Unhover
    fireEvent.mouseLeave(alert);

    // Advance remaining ~3000ms
    act(() => {
      vi.advanceTimersByTime(3100);
    });

    // Message should now be dismissed
    expect(screen.queryByText('Hover test message')).not.toBeInTheDocument();
  });
});
