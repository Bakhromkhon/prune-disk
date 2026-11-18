# Prune: product art direction

## Why another pass

The first preview and its accessibility refinements share essentially the same
composition. Better copy and contrast improved usability but did not provide the
distinct visual product direction the user requested. This pass evaluates
composition, not another recolor.

## Non-negotiables

- Flat surfaces: no shadows, gradients, glass, decorative glows or KPI-card grids.
- A recognizable local utility, not a marketing page or generic SaaS dashboard.
- Russian plain-language labels, system fonts, intentional density and whitespace.
- A report is not a whole-disk measurement. No invented capacity/free-space chart.
- Demo data remain visibly fictional. Real imports remain local.
- No fake scanning, cleanup, restore or history actions.
- Existing validation, escaping, privacy restrictions and risk caveats remain.
- Accessibility checks are not traded away for a distinctive silhouette.

## Two directions to compare

**Atlas**: category rail, a single proportional report overview, ranked objects
and an inspector. A tool for understanding the structure of a report.

**Focus**: no sidebar; a quieter two-pane ranked list with contextual explanations.
A tool for reading one object at a time without technical clutter.

Both use the same report schema and fabricated data. They are interactive design
concepts alongside the existing viewer, not two supported production applications.
Select a direction before replacing the primary viewer and consolidating code.

## Reference principles, not copied assets

- [DaisyDisk](https://daisydiskapp.com/): an overview is the central interaction,
  rather than ornamental charts around a table. Its published product screenshot
  was inspected. We do not copy its radial map, interface assets or safety claims.
- [Pearcleaner, author-owned repository](https://github.com/alienator88/Pearcleaner):
  selection and associated objects are directly connected. Use the author's
  repository, not unaffiliated download websites; no source/assets are copied.
- [Apple Storage settings](https://support.apple.com/guide/mac-help/change-storage-settings-mchl3d437fbc/mac):
  category-based progressive disclosure is a reference principle from search
  excerpts. The guide URL did not load during this pass; no screenshot inspection
  or detailed behavior verification is claimed.

This is a qualitative reference study, not a competitive benchmark or proof that
Prune is superior to other products.

## Pre-selection acceptance (completed)

1. The visual difference must be obvious at the same desktop viewport, with the
   same demo report—not depend on changed color or font size alone.
2. A first-time viewer can identify the source, largest object and next available
   action without assuming this page scans or deletes.
3. Selection, search, filters and local import work, not just look interactive.
4. Inspect a real browser screenshot; verify keyboard behavior, contrast and
   overflow separately. Mark mobile/zoom/screen-reader gaps explicitly.
5. Present both alternatives and the previous viewer for comparison. Avoid
   calling either concept finished before user evaluation.

## Selected iteration: Atlas

The user approved applying the project-local `frontend-design` skill. Atlas is
the primary report-preview direction; Focus remains available as an alternate
concept. The viewer remains a local read-only web preview, not a shipped Tauri app.

**Problem and verification:** at 816×616 CSS px the first Atlas pass used about
254 px for its overview and showed no complete result rows. The refined pass uses
106 px and shows three complete rows. The live viewer was also checked at 1000×700.

**Composition:** compact report context above the list, category navigation on the
left (or a compact strip), and persistent object explanation on the right. No
marketing hero, decorative sequence numbers or all-caps pseudo-metadata.

**Visual intent:** mineral-white utility surfaces, readable graphite text and a
forest accent reserved for selection/actions. System fonts suit familiar local
file navigation; no external font loading is needed. Deliberate spacing and
content hierarchy, not ornamental styling, carry the identity.

**Acceptance:** at the same 816×616 viewport, at least three complete file rows
must be visible alongside the inspector. Text stays at least 12 CSS px; controls
remain usable, and errors/partial status must not be hidden to achieve density.
Actual browser geometry, screenshot availability and keyboard results must be
reported separately from unit-test success.

The structural/density acceptance is met for the fictional complete report.
Partial reports can consume more vertical space for warnings. Screen-reader,
200% zoom and multiple physical-device checks remain open; this is not a WCAG
conformance claim or nontechnical-user study.
