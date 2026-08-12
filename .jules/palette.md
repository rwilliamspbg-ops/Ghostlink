# Palette's Journal

## 2026-08-10 - [Model Selector Download Redirection & Focus Visibility]
**Learning:** Custom drop-down popups (like the Model Selector dropdown) that contain helpful quick-links (like "+ Download Models") are highly frustrating when they act as non-functional static links. Making them interactive by wiring them to programmatically switch tabs (`setActiveTab(1)`) greatly reduces cognitive friction. Additionally, aligning standard form inputs (like the MCP 'requires_confirmation' checkbox and popular model card buttons) with high-contrast, accessible `focus-visible:` keyboard rings guarantees keyboard and screen-reader navigators do not lose context in dark-theme interfaces.
**Action:** Always ensure helper actions in dropdowns are functional and correctly redirect to their corresponding full interface tab, and verify all checkboxes/buttons have consistent, high-contrast focus rings.

## 2026-08-09 - [MCP Dialog Input Forms Accessibility & Micro-UX]
**Learning:** Dialog entry forms (such as the MCP server creation and edit form) that have dense parameters benefit immensely from explicit required marks (`*`), contextual placeholders, clear tooltips via `title` attributes, and `cursor-pointer` label target enhancement for checkboxes and radio fields. Providing clear `focus-visible:` keyboard rings across all text inputs, textareas, radios, and checkboxes ensures users are never visually lost during dense forms setup.
**Action:** Always mark mandatory form inputs explicitly, widen interactive click targets, and add detailed placeholders, informative tooltips, and high-visibility keyboard focus rings to interactive forms.

## 2026-08-08 - [Markdown Code Blocks and Compare Mode Copy Tooltips and Focus Rings]
**Learning:** Core user utilities such as copy-to-clipboard buttons (both nested inside markdown code blocks and listed within Compare Mode column segments) are easily lost during keyboard navigation if they lack distinct, high-contrast focus visible indicator classes. Sighted desktop users expect visual hover confirmation via browser `title` tooltips, while keyboard/assistive navigators require the elements to become visually apparent and highlighted (e.g. `focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-blue-500`) to track active cursor context correctly.
**Action:** Always provide custom high-contrast focus outlines and descriptive `title` attributes on all localized, action-oriented clipboard utility trigger elements.

## 2026-08-07 - [Metrics Tab Header Accessibility]
**Learning:** Live monitoring headers often contain specialized quick-action buttons (like Export CSV and Manual Refresh) that lack text labels to keep the dashboard visual design minimal. While they have correct `aria-label` and `title` attributes, they must also be styled with customized, high-contrast focus rings (`focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none`) to preserve keyboard navigation. Without explicit indicators, users tabbing through live performance monitors will lose their selection completely, severely degrading the accessibility of critical dashboard features.
**Action:** Always complement icon-only action triggers in live monitors and charts with explicit focus rings to guarantee seamless, visual keyboard tracking.

## 2026-08-06 - [Workspace & Editor Tab Keyboard Navigation]
**Learning:** Development environments and files workspaces (such as the Editor tab with its file tree checkbox selectors, expandable folders, workspace-level toggles, and edit actions) need high-visibility focus indicator rings (`focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none`). Relying on default browser focus states in dark-themed, nested pane designs leaves keyboard-only navigators or assistive technology users completely lost, unable to determine which button, input, or checkbox is currently active.
**Action:** Ensure all workspace file checkboxes, file/folder tree nodes, editor action triggers, and diff confirmation buttons are styled with explicit, high-contrast focus visible styles to maintain continuous keyboard-accessible visual feedback.

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

## 2026-08-11 - [Settings Save and Reset Toast Notifications Feedback]
**Learning:** Forms managing critical application-wide or cluster settings must provide prominent, high-fidelity feedback (such as transient toast alerts) for both success and failure outcomes during actions like "Save" and "Reset to Defaults". Failing to do so can result in silent errors when API requests fail, leaving users completely unaware that their modifications or resets were rejected.
**Action:** Always integrate standard configuration-saving and state-reset operations with global/app-wide toast notification systems to guarantee visibility of results.
