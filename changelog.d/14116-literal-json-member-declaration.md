<!-- section: Fixed -->
- Literal `json!` construction (RIPR-SPEC-0197) is now admitted in a virtual
  workspace whose member declares `serde_json` itself, instead of only when
  the workspace root manifest names it. The importing package's resolved
  dependency edge, audited release and checksum are still required, so a
  member without that dependency keeps the refusal (#14116).
