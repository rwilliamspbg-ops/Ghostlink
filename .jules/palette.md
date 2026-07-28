# Palette's Journal

## 2026-07-20 - [Clear Chat Confirmation & Tooltip]
**Learning:** Destructive actions placed immediately adjacent to common utility actions (like Save and Load) without tooltips or confirmation dialogs lead to high rates of accidental data loss and user frustration. Sighted users need hover tooltips (`title`) to understand icon-only buttons, and all users benefit from a non-blocking confirmation dialog before clearing an active session.
**Action:** Always add a clear confirmation dialog and descriptive tooltips to header actions that discard user-generated state.

## 2026-07-21 - [Disconnect Worker Confirmation]
**Learning:** Instantly triggering disruptive cluster actions (like disconnecting a worker node) on single click without warning leads to operator frustration and unstable network environments. Adding a standard confirmation dialog before executing the action prevents accidents and provides peace of mind.
**Action:** Always prompt for confirmation before executing any state-destructive or cluster-disruptive operations.
