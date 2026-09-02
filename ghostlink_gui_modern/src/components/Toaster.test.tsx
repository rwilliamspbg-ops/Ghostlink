import React from 'react';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { describe, expect, it, beforeEach } from 'vitest';
import { Toaster } from './Toaster';
import { useAppStore } from '../store';

describe('Toaster', () => {
  beforeEach(() => {
    act(() => {
      useAppStore.setState({ toasts: [] });
    });
  });

  it('renders nothing when there are no toasts', () => {
    const { container } = render(<Toaster />);
    expect(container.firstChild).toBeNull();
  });

  it('renders toast alerts with role="alert" and aria-label on toaster container', () => {
    act(() => {
      useAppStore.getState().addToast({ type: 'success', message: 'Model downloaded successfully' });
    });

    render(<Toaster />);

    const container = screen.getByLabelText('Notifications');
    expect(container).toBeInTheDocument();
    expect(container).toHaveAttribute('aria-live', 'polite');

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
});
