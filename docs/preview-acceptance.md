# Report viewer: acceptance criteria

The viewer is an offline presentation adapter for scan reports. It is not a
desktop scanner, a cleanup executor or evidence that a file may safely be removed.

## First-use understanding

- At every viewport size, the main content explains that this page opens an
  existing Prune report and does not scan or delete files.
- Fictional demo data cannot be mistaken for a scan of the current computer.
- Opening a report is the primary action. File format, size limit and local-only
  processing are explained before opening the picker.
- Instructions for obtaining a report describe the actual CLI, not a simulated
  scanning button.

## Content and data

- File sizes are not described as freed or guaranteed reclaimable storage.
- Risk is identified as an estimate from the report, not permission to delete.
- Re-creating data is not described as recovering the original contents.
- Partial/cancelled reports and omitted errors are explicit.
- Empty reports, empty filtered results and incomplete reports without findings
  have different explanations.
- A failed import leaves the prior report visible, with this stated explicitly.
- Imported paths and other strings remain text, never executable HTML.

## Interaction

- Filtering, sorting and pagination keep details consistent with a visible row.
- Selection alone does not unexpectedly steal keyboard focus.
- Explicit controls can move focus to details and back.
- Pagination announces the page number and does not strand focus on a disabled
  button.
- Search/filter reset is a real operation, not decorative UI.

## Visual/accessibility checks

- Ordinary text contrast at least 4.5:1; meaningful field boundaries at least 3:1.
- Metadata no smaller than 12 CSS px; ordinary text and controls at least 14 px.
- Test 375, 768, 1024 and 1440 CSS px widths; contain table overflow without
  document-level horizontal scrolling.
- Check 200% text/zoom, keyboard focus visibility and screen-reader announcements
  in an actual browser. Node DOM fixtures do not establish these properties.
- Risk and report state must remain understandable without color.

## Release evidence

Record which checks were executed and which remain manual. Passing automated
tests or a source audit is not a declaration of complete WCAG conformance or
successful usability research with nontechnical users.

## Current refinement evidence

- Independent source/DOM review passed; numeric CSS contrast was checked.
- Real-browser check at an 816 CSS px viewport: no document-level horizontal
  overflow, no console errors, visible controls were 44 px tall, and explicit
  detail/back actions moved focus as intended.
- A browser screenshot was captured for visual inspection.
- Full multi-width, 200% zoom and real screen-reader acceptance remain open.
