<!-- section: Fixed -->
- LSP: saving a TypeScript or JavaScript file that no longer parses no longer
  leaves the editor on a clean-looking list and a "no related tests found"
  code lens. The file now carries the same passive Information marker as an
  unparseable Rust or Python file ("ripr could not fully parse this file, so
  findings and related tests for it can be missing; having no findings here
  does not mean it is clean"), with the limitation's own next step, and the
  lens withholds its no-tests claim. The marker carries no finding identity,
  repair data or actionability. Other `language_scope_unsupported` limitations
  (read refusals, non-UTF-8 files) are not named parse failures.
  `ripr/analysisStatus` and `ripr check` are unchanged (#13371).
