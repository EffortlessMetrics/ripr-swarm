<!-- section: Internal / CI -->
- `ripr` memoizes the macro-rebound scan per requested name set, so probe-heavy analyses share one parser-backed pass instead of rescanning the corpus per probe (#10977).
