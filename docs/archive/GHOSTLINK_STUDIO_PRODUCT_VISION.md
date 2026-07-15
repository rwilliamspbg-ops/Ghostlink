# Ghostlink Studio Product Vision

Ghostlink Studio is a standalone desktop app that makes distributed inference on spare GPUs feel as simple as a local chat app.

## Experience Goals

- One-click startup with safe defaults
- Visual cluster health and throughput at a glance
- Fast model-to-chat workflow for non-technical users
- Advanced controls available without cluttering the primary flow

## UX Design Direction

- Dark, modern desktop-first interface with clear hierarchy
- Left navigation rail: Home, Models, Chat, Cluster, Settings, Doctor
- Center panel: core workflow and metrics
- Right panel: contextual details and controls
- Progressive disclosure: simple by default, advanced when requested

## MVP Feature Scope

### Home

- Cluster online status
- Total VRAM and active sessions
- Throughput and latency metric cards
- Quick actions: Start Cluster, Stop All, Run Flow, Open Doctor

### Models

- Local model inventory and compatibility status
- One-click download and quantization actions
- Int4 and Int8 entry points tied to existing runtime capabilities

### Chat

- Chat UI with streaming responses
- Model and parameter selector
- Automatic backend selection (single-node vs distributed)

### Cluster

- Node cards with GPU and host information
- Health states (green/yellow/red)
- Real-time load distribution summary
- Placement preview for layer distribution

### Settings

- TOML config editor
- Discovery and networking controls
- Preset profiles: Speed, Balanced, Max Quality

### Doctor

- One-click diagnostics run
- Structured categories and actionable fix suggestions
- JSON export and copy-ready remediation commands

## Architecture Recommendation

- Tauri 2 for desktop shell and Rust command bridge
- Frontend implementation in SvelteKit for responsive UI and fast iteration
- Reuse existing Ghost-Link CLI/core code through thin command adapters

## Delivery Phases

- Phase 1 (Weeks 1-6): MVP shell, dashboard, models, chat, cluster, settings, doctor
- Phase 2 (Weeks 7-12): server mode, multi-user support, history graphs, import/export, accessibility, auto-update
