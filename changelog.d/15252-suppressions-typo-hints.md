<!-- section: Fixed -->
- A misspelled `.ripr/suppressions.toml` entry header (`[[suppression]]`, `[suppressions]`), field (`functon`) or `kind` (`exposure-gap`) now names the valid spelling, and keys under an unknown header no longer cascade into unrelated top-level field errors; `[[ suppressions ]]` and a trailing comment after the header are accepted (#15252).
