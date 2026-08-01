# Palette's Journal

## 2026-07-20 - [Clear Chat Confirmation & Tooltip]
**Learning:** Destructive actions placed immediately adjacent to common utility actions (like Save and Load) without tooltips or confirmation dialogs lead to high rates of accidental data loss and user frustration. Sighted users need hover tooltips (`title`) to understand icon-only buttons, and all users benefit from a non-blocking confirmation dialog before clearing an active session.
**Action:** Always add a clear confirmation dialog and descriptive tooltips to header actions that discard user-generated state.

## 2026-07-21 - [Disconnect Worker Confirmation]
**Learning:** Instantly triggering disruptive cluster actions (like disconnecting a worker node) on single click without warning leads to operator frustration and unstable network environments. Adding a standard confirmation dialog before executing the action prevents accidents and provides peace of mind.
**Action:** Always prompt for confirmation before executing any state-destructive or cluster-disruptive operations.

## 2026-07-22 - [Empty Chat Onboarding Suggestion Chips]
**Learning:** Empty conversational states create a high cognitive load for users who may not know how to start. Providing pre-configured prompt suggestion buttons under the welcome message reduces onboarding friction. Ensuring these buttons are semantic elements (`<button>`), fully accessible with descriptive ARIA labels, and focus-linked to the main chat composer creates a smooth, intuitive keyboard and mouse experience.
**Action:** Always include interactive, accessible suggestion cards/chips in blank conversational views to guide users and focus the text composer when selected.

## 2026-07-23 - [Keyboard Navigation Focus Rings]
**Learning:** Custom interactive elements (such as onboarding suggestion buttons, icon-only header utility buttons, and custom model dropdown triggers) that rely on default focus outlines often have their outlines swallowed by absolute wrappers, overflow constraints, or dark-theme backgrounds. This makes keyboard-only and screen-reader navigation completely blind and confusing. Using specific Tailwind focus-visible styling ensures clear, localized, high-contrast focus rings without cluttering the visual layout for mouse/touch-first users.
**Action:** Always apply tailored focus-visible indicators (`focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none`) to all custom-styled buttons, chips, and dropdown triggers to preserve keyboard accessibility.

## 2026-07-31 - [MCP Tab Keyboard Accessibility]
**Learning:** Custom toggle buttons and control headers in utility panels (like the MCP Servers tab) must have high-contrast focus-visible styles configured. Without them, keyboard users can navigate to the page but are left unable to clearly discern which server toggle or action button currently holds visual focus.
**Action:** Ensure all button elements inside toggle lists and header actions use `focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none` for excellent keyboard navigation visibility.
