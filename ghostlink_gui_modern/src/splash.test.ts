import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { dismissSplash } from './splash';

describe('dismissSplash', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    document.body.innerHTML = '<div id="splash"><div class="bg-grid"></div></div><div id="root"></div>';
  });
  afterEach(() => vi.useRealTimers());

  it('fades then removes the whole overlay, including background layers', () => {
    dismissSplash(document, 250);
    expect(document.getElementById('splash')?.classList.contains('fade-out')).toBe(true);
    vi.advanceTimersByTime(250);
    expect(document.getElementById('splash')).toBeNull();
    expect(document.querySelector('.bg-grid')).toBeNull();
    expect(document.getElementById('root')).not.toBeNull();
  });

  it('is a no-op when the splash is already gone or already dismissing', () => {
    dismissSplash(document, 250);
    expect(() => dismissSplash(document, 250)).not.toThrow();
    vi.advanceTimersByTime(250);
    expect(() => dismissSplash(document, 250)).not.toThrow();
  });
});

describe('index.html boot shell', () => {
  const html = readFileSync(resolve(__dirname, '../index.html'), 'utf-8');

  it('keeps the splash out of the flow shared with #root', () => {
    expect(html).toMatch(/\.splash-overlay\s*{[^}]*position:\s*fixed/);
    const body = html.match(/\n\s*body\s*{([^}]*)}/)[1];
    expect(body).not.toMatch(/display:\s*flex/);
    // splash must be a child of the overlay, and #root must not be inside it
    const overlay = html.indexOf('id="splash"');
    expect(overlay).toBeGreaterThan(-1);
    expect(html.indexOf('class="splash"')).toBeGreaterThan(overlay);
    expect(html.indexOf('id="root"')).toBeGreaterThan(html.indexOf('class="splash"'));
  });

  it('does not remove the splash on a fixed timer', () => {
    expect(html).not.toMatch(/splash\.remove\(\)\s*,\s*500/);
  });
});
