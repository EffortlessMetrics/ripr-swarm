<!-- section: Changed -->
- `ripr check` no longer re-runs the rust-analyzer syntax validation pass (`Parse::errors`) every time it asks whether an already-parsed Rust file is clean: the error count is computed once per distinct parse and kept with the per-thread parse memo, so memo hits and the file-fact summarizer reuse it. Findings and JSON output are unchanged (#11205).
