<!-- section: Internal / CI -->
- `cargo xtask eval-sweep refresh` rejects malformed analyzer output before
  completeness and stability scoring, keeps the failure reason in execution
  receipts, and preserves valid empty findings and exact first-pass raw bytes (#13132).
