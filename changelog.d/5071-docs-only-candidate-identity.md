<!-- section: Fixed -->
- Docs-only routed Rust classification now compares the immutable PR event
  base/head commits, keeping unrelated main changes in the integration checkout
  from widening the PR's changed-file set. Unavailable or ambiguous history and
  failed or empty diffs retain full-proof routing (#5071).
