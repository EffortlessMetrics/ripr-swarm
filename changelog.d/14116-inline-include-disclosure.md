<!-- section: Fixed -->
- Rust assertion refusal evidence now names the first workspace-reaching
  `include!` that makes placement unavailable instead of blaming an unrelated
  glob, and only when the glob is private, unscoped and would otherwise be
  module-confined: a public glob keeps its direct explanation. Pins the
  already-working inline-only SCIP fragment control (#14116).
