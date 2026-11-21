<!-- section: Fixed -->
- A literal `serde_json::json!` call in a virtual workspace (a root
  `Cargo.toml` with only `[workspace]`, whose member declares `serde_json`)
  is now verified against the audited registry release like one in a single
  package. Before, only a root that declared `serde_json` itself qualified,
  so on such workspaces a `json!` call that mentions `format` or `vec` kept
  every test that uses those macros uncredited. Globbed or escaping member
  paths stay unverified (RIPR-SPEC-0197, #14116).
