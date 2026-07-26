# Palette's Journal

## 2026-07-20 - [Clear Chat Confirmation & Tooltip]
**Learning:** Destructive actions placed immediately adjacent to common utility actions (like Save and Load) without tooltips or confirmation dialogs lead to high rates of accidental data loss and user frustration. Sighted users need hover tooltips (`title`) to understand icon-only buttons, and all users benefit from a non-blocking confirmation dialog before clearing an active session.
**Action:** Always add a clear confirmation dialog and descriptive tooltips to header actions that discard user-generated state.
