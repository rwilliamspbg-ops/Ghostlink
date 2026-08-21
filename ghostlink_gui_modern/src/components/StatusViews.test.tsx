import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { AlertCircle, Inbox, Plus } from 'lucide-react';
import { EmptyState, ErrorPanel, LoadingState } from './StatusViews';

describe('StatusViews', () => {
  it('renders LoadingState with correct ARIA attributes and motion-safe spin class', () => {
    render(<LoadingState label="Fetching models..." />);
    const statusEl = screen.getByRole('status');
    expect(statusEl).toHaveAttribute('aria-live', 'polite');
    expect(statusEl).toHaveAttribute('aria-busy', 'true');
    expect(screen.getByText('Fetching models...')).toBeInTheDocument();

    const loaderSvg = statusEl.querySelector('svg');
    expect(loaderSvg).toHaveClass('motion-safe:animate-spin');
  });

  it('renders ErrorPanel with role alert and aria-live assertive', () => {
    render(<ErrorPanel icon={AlertCircle} title="Failed to load" message="Network timeout occurred." />);
    const alertEl = screen.getByRole('alert');
    expect(alertEl).toHaveAttribute('aria-live', 'assertive');
    expect(screen.getByText('Failed to load')).toBeInTheDocument();
    expect(screen.getByText('Network timeout occurred.')).toBeInTheDocument();
  });

  it('renders EmptyState with title and description', () => {
    render(<EmptyState icon={Inbox} title="No items found" description="Try refining your filter." />);
    expect(screen.getByText('No items found')).toBeInTheDocument();
    expect(screen.getByText('Try refining your filter.')).toBeInTheDocument();
  });

  it('renders EmptyState with action button and handles click', () => {
    const handleClick = vi.fn();
    render(
      <EmptyState
        icon={Inbox}
        title="No workers"
        description="Connect a node."
        action={{ label: 'Add Worker', onClick: handleClick, icon: Plus }}
      />
    );
    const buttonEl = screen.getByRole('button', { name: 'Add Worker' });
    expect(buttonEl).toBeInTheDocument();
    expect(buttonEl).toHaveClass('focus-visible:ring-2');
    fireEvent.click(buttonEl);
    expect(handleClick).toHaveBeenCalledTimes(1);
  });
});
