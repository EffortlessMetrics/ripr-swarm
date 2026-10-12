<!-- section: Fixed -->
- A uniqueness-only helper-chain stop no longer rewrites `no_static_path`
  to `static_unknown` when tests only call a type-qualified sibling of
  the uniqueness-stop hop (`B::parse` when the hop is `A::parse`). Bare
  `parse(...)` and a call that targets the hop still enter (#7268).
