## 2025-05-18 - GaugeCard Accessible Values & Tooltips
**Learning:** Gauge progressbars with only `aria-valuenow` integers lack exact precision in screen readers when floating-point percentages are displayed visually; adding `aria-valuetext` ensures clear audible percentage reading while `title` provides hover tooltips.
**Action:** When creating radial or linear progressbars with decimal values or subtitles, provide both `aria-valuetext` and a contextual native `title` attribute.
