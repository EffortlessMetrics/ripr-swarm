<!-- section: Added -->
- `ripr check --format json` now embeds the same bounded
  `canonical_next_action` object the human triage renderer uses, including
  the selected item, effective diff source, and followable inspect route
  (#7258). Machine-readable check results no longer omit that next action.
  The generic JSON renderer and unbounded `pr-evidence` path still omit it
  because they have no navigation/provenance context.
