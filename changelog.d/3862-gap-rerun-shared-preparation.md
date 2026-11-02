<!-- section: Changed -->
- `ripr rerun --gap` prepares the Rust workspace and cache-backed index once per
  invocation when several ledger scopes need Rust recomputation, and reports
  shared file-fact cache work once instead of once per scope (#3862).
