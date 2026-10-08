/**
 * Removes the static splash overlay that index.html renders before React
 * hydrates. Called from main.tsx once the app has painted, so the splash lasts
 * exactly as long as boot does instead of a fixed timer.
 */
export function dismissSplash(doc: Document = document, fadeMs = 250): void {
  const el = doc.getElementById('splash');
  if (!el || el.classList.contains('fade-out')) return;
  el.classList.add('fade-out');
  setTimeout(() => el.remove(), fadeMs);
}
