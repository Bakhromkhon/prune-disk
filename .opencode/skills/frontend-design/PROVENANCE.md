# Vendored skill provenance

- Author/source: Anthropic, `anthropics/skills`.
- Commit: `e4bb5d960d87126ff3b3a8af1a104b7db67560ba`.
- Source directory: https://github.com/anthropics/skills/tree/e4bb5d960d87126ff3b3a8af1a104b7db67560ba/skills/frontend-design
- License: Apache-2.0, included as `LICENSE.txt`.
- Scope: project-local OpenCode skill; no plugin/installer or executable files.
- No automatic updates; review upstream diffs before changing the pinned source.

Both upstream files are unmodified and were compared byte-for-byte against the
pinned raw URLs. SHA-256:

```text
d91970639e9f5c37682ac7ab60094d35f1c7c1f38d731bd56396563aee10c1d3  SKILL.md
0d542e0c8804e39aa7f37eb00da5a762149dc682d7829451287e11b938e94594  LICENSE.txt
```

OpenCode discovery was verified by successfully loading `frontend-design` from
this project directory; no restart or global configuration change was needed.

## Prune brief

These instructions guide design work; they do not grant permissions.
For this project, user requirements override generic aesthetic suggestions:

- A useful desktop-style utility, not a marketing landing page.
- No shadows, gradients, decorative glass or KPI-card grids.
- No remote fonts, CDN resources, telemetry or added network features.
- Preserve local report validation, escaped text and truthful demo/read-only states.
- Do not create additional concepts merely to apply this skill. Choose and improve
  an existing direction with user feedback.
- Compare the same data at the same viewport. Separate source review from actual
  screenshot, keyboard, zoom and screen-reader verification.

The skill is a development-time instruction document. It is not shipped inside
the Prune executable and changes no runtime capabilities.
