# UI Polish Sprint (Focused Pass)

Date: 2026-07-05
Scope: Tauri/Svelte Studio frontend visual and interaction polish in one PR-sized iteration.

## Sprint Goals

- Tighten visual system: typography, spacing, card rhythm, and hierarchy.
- Improve Chat and Workers information layout.
- Add responsive behavior and explicit loading states.
- Align branding and icon usage across navigation and shell.
- Ship with visual evidence and acceptance checklist.

## What Changed

### Visual System

- Unified left-rail branding with a consistent mark + subtitle treatment.
- Improved nav item hierarchy with icon, title, and descriptor.
- Tightened spacing rhythm and card cadence across shell, hero, and content zones.
- Added details-panel runtime state chip (`READY` / `RUNNING`).
- Added top-level busy banner and subtle motion cues.

### Chat and Workers Hierarchy

- Chat tab moved to a two-column composition:
  - Left: Prompt Builder (model/prompt/controls)
  - Right: Response card + Recent Exchanges stack
- Cluster tab now includes KPI cards:
  - discovered workers
  - reachable workers
  - selected workers
  - selected and reachable workers
- Batch and worker operation buttons now show clearer progress text states.

### Responsive and Loading States

- Added refined breakpoints for:
  - <= 1400px: tighter tri-column shell and KPI redistribution
  - <= 1100px: stacked shell with single-column content grids
  - <= 820px: compact nav, full-width controls, mobile-focused spacing
- Added loading skeleton cards for startup metric snapshot.
- Added explicit busy status messaging in-shell while commands execute.

### Branding and Icon Consistency

- Added iconography to every nav entry with consistent visual casing.
- Standardized brand styling and shell-level accent usage.
- Added preview-mode banner for non-Tauri mock bridge sessions.

### Screenshot Capture Tooling

- Added a reproducible screenshot script for this UI:
  - `npm run screenshots:ui`
- Added Playwright as frontend dev dependency.
- Added mock bridge support (`?mock=1`) so screenshots are deterministic outside Tauri runtime.

## Screenshots

- Home desktop: [docs/screenshots/ui-polish/01-home.png](screenshots/ui-polish/01-home.png)
- Chat desktop: [docs/screenshots/ui-polish/02-chat.png](screenshots/ui-polish/02-chat.png)
- Cluster desktop: [docs/screenshots/ui-polish/03-cluster.png](screenshots/ui-polish/03-cluster.png)
- Home mobile: [docs/screenshots/ui-polish/04-mobile-home.png](screenshots/ui-polish/04-mobile-home.png)

## Acceptance Checklist

- [x] Visual system tightened (type rhythm, spacing, card cadence)
- [x] Chat hierarchy improved with clearer builder/output separation
- [x] Workers hierarchy improved with KPI summary and clearer controls
- [x] Responsive breakpoints implemented and validated with mobile screenshot
- [x] Loading states added (busy banner + startup skeleton cards)
- [x] Branding and icon consistency improved across shell/nav
- [x] Build passes: `npm run build`
- [x] Screenshot artifacts generated and committed

## Reproduce

```bash
cd crates/ghostlink-gui/frontend
npm install
npm run build
npm run preview -- --host 127.0.0.1 --port 4173
npm run screenshots:ui
```

Notes:
- Screenshot capture uses mock bridge mode (`?mock=1`) for deterministic visual output.
- Runtime functionality remains connected to real Tauri `invoke` in normal app execution.
