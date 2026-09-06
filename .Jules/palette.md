## 2026-09-06 - Link Form Descriptions and Warnings via `aria-describedby`
**Learning:** Screen reader users navigating through complex settings forms via Tab focus do not automatically hear field descriptions or inline warning banners unless the input elements explicitly reference them using `aria-describedby` and `aria-labelledby`.
**Action:** In multi-field form components with helper text, generate deterministic IDs (e.g. `${fieldId}-desc`, `${fieldId}-warning`) and attach `aria-describedby` to `input`, `select`, and `radiogroup` containers.
