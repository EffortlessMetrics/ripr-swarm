<!-- section: Fixed -->
- `ripr agent brief --diff` and `--base` refuse shared-parser limitations and
  admission errors with the typed `diff_input_unsupported` identity
  instead of emitting a successful empty or repository-fallback brief;
  valid empty and ordinary patches retain their behavior (#7331).
