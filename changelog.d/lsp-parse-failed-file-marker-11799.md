<!-- section: Fixed -->
- LSP: saving a Rust file that no longer parses no longer leaves the editor on
  an empty diagnostic list that reads as clean. A changed Rust file with a
  syntax or nesting error or non-UTF-8 source, or a Python file with a syntax
  error, now carries one passive Information marker under the existing
  governed `ripr-diagnostic-language-not-analyzed` code ("ripr could not fully
  parse this file, so findings and related tests for it can be missing; having
  no findings here does not mean it is clean"), with the
  limitation's own next step. The marker carries no finding identity, repair
  data or actionability, and the code lens already withholds its "no related
  tests found" claim for such a file (#12020). `ripr/analysisStatus` and
  `ripr check` are unchanged (#11799).
