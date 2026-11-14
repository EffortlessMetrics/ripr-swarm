<!-- section: Internal / CI -->
- `cargo xtask check-fast` scopes local Clippy to the packages impacted by the committed diff (including reverse dependents), widening to the workspace for shared surfaces, unknown owners, or an incomplete diff, and reports the impacted plan plus tests not run instead of claiming full readiness (#14644).
