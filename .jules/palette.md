# Palette's Journal

## 2026-08-05 - [Sidebar Search Button & Shortcut Badges]
**Learning:** Displaying incorrect hotkey badges in primary UI controls (such as labeling "New Chat" with "Ctrl+K" instead of "Ctrl+Shift+O") confuses power users and misrepresents keyboard capability. Additionally, hidden features (like a global Command Palette) are significantly more accessible and discoverable when represented by a clear, dedicated visual trigger button in main navigation menus. Custom programmatic event systems with active focus restoration allow seamless visual triggers without losing screen-reader context.
**Action:** Always provide dedicated visual trigger buttons for hidden features like command palettes, label shortcut badges precisely, and use custom window events with focus restoration to trigger them smoothly.

## 2026-08-04 - [Settings Safety and Accessibility Controls]
**Learning:** Destructive utility actions in complex dashboards (like resetting runtime configurations) that are positioned directly adjacent to primary actions (like Save) must be guarded with descriptive titles and modal confirmation dialogs (`window.confirm`) to prevent catastrophic data loss. Furthermore, inputs, sliders, and buttons must utilize explicit high-contrast focus rings (`focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none`) to ensure keyboard navigatees have complete visual tracking on dark-theme UI layers.
**Action:** Always secure configuration reset triggers with clear, modal confirmation prompts, native informational tooltips, and high-visibility keyboard focus state rings.

## 2026-08-02 - [Sessions Active Protection & Tooltips]
**Learning:** Cancelling a running LLM inference session is a highly disruptive action. Sighted users need immediate visual hover tooltips (`title`) to confirm the destructive function of icon-only action triggers, and all users benefit from a non-blocking confirmation dialog (`window.confirm`) to prevent accidental session evictions. Additionally, ensuring these interactive controls have tailored keyboard focus rings guarantees an accessible experience during keyboard navigation.
**Action:** Always protect cluster session teardowns with clear modal confirmations, intuitive tooltips, and tailored `focus-visible:` focus rings.

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

## 2026-08-01 - [Interactive Controls Keyboard Visibility]
**Learning:** Interactive controls like tab lists (e.g., in the Models Dashboard) and control action buttons/inputs (e.g., inside the Cluster Workers dashboard) must use high-contrast focus-visible rings. Relying on default focus styles leads to invisible focused items during tab traversal on dark-theme UI panels, leaving keyboard and screen-reader users completely blind.
**Action:** Always style all dashboards' custom buttons, tab toggles, and input elements with `focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none`.
