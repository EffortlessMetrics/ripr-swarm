<!-- section: Fixed -->
- Checking whether a `serde_json::json!` call names the audited registry
  release no longer rereads the Cargo configuration for every macro call it
  meets. Each file's answer is now worked out once per analysis. On a
  workspace with many `json!` calls the old way made `ripr check` up to ten
  times slower (RIPR-SPEC-0197, #16038).
