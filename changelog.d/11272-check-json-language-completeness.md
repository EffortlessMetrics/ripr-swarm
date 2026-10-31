<!-- section: Added -->
- `ripr check --json` now carries an additive `language_completeness` member: the
  shared per-language completeness view (status, finding count, limitations and
  disclosure route per language). A completed zero-finding run
  (`complete_zero`) is now distinguishable from a skipped or unavailable adapter
  without reading the editor-only view. No existing field changes. #11272.
