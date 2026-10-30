<!-- section: Fixed -->
- `cargo clippy -D warnings` passes again: the two intentional readonly-flag
  clearings in the agent workflow-artifact cleanup carry scoped, reasoned
  lint allowances instead of tripping `permissions_set_readonly_false`
  (#11282).
