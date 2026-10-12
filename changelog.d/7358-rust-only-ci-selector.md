<!-- section: Fixed -->
- Rust-only CI selection uses the pull request event base and head, so unrelated
  changes on main do not select Rust feature tests for a documentation-only or
  workflow-only contribution. Classification preserves renamed and deleted paths
  and selects the Rust lane conservatively when comparison is unavailable (#4987).
