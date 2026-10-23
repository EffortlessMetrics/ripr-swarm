<!-- section: Fixed -->
- A default-base `ripr check` whose range has no analyzable changed files (for
  example a branch that only deletes a file) no longer tells the reader that no
  behavior changed. The human note, the GitHub `ripr no analysis scope`
  annotation and the JSON `scope_disclosures[].why` now say the range has no
  analyzable changed files, that deleted files, submodule pointer changes and
  files ripr does not analyze are not counted, and that the empty result does
  not mean no behavior changed. `why` remains advisory prose; no key or schema
  version changes (#9222, #9033).
