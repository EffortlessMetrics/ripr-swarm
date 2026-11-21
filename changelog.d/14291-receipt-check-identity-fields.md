<!-- section: Fixed -->
- `ripr receipt check` no longer reports a receipt as structurally valid when its `kind`, `tool` or `schema_version` names another record (an agent receipt, another tool, an unknown or future schema): it exits non-zero, naming the field, the expected value and how to rewrite it (#14291).
