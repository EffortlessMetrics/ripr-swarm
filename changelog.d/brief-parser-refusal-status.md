<!-- section: Fixed -->
- `ripr agent brief --diff` and `--base` refuse shared-parser limitations and
  admission errors and truncated textual sections with the typed `diff_input_unsupported` identity
  instead of emitting a successful empty or repository-fallback brief;
  valid empty and ordinary patches retain their behavior (#7331).
- Diff path-depth admission follows hunk boundaries so slash-heavy added or
  removed source lines are not refused as path headers; actual path headers
  retain their existing bounds (#8961).
