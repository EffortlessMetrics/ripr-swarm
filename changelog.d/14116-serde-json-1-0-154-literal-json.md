<!-- section: Fixed -->
- Literal `json!` construction (RIPR-SPEC-0197) is now admitted for serde_json
  1.0.154 as well as 1.0.151. Its `json!` macro source is unchanged from
  1.0.151; each release is accepted only with its own crates.io checksum and
  the existing resolved-edge checks, and any other release keeps the refusal
  (#14116).
