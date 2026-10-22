<!-- section: Internal / CI -->
- The termination qualification harness now also runs the path-glob corpus:
  many-star and many-segment nonmatches through `--suppression-policy`
  selectors and `generated_file_patterns`, with classifier arms and
  stub-binary end-to-end controls. No product behavior changes (#4513).
