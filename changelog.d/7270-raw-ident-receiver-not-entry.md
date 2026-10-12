<!-- section: Fixed -->
- A uniqueness-only helper-chain stop no longer rewrites `no_static_path`
  to `static_unknown` when tests only call a raw-identifier receiver or
  foreign path that shares the ambiguous name (`req.r#parse()`,
  `foreign::r#parse()`). Bare `r#parse(...)` still enters as the same
  identifier as `parse(...)` (#7270).
