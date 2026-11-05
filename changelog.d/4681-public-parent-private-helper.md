<!-- section: Fixed -->
- Rust analysis keeps the public-parent to private-helper proof path when a helper moves into an inline, file-backed or parent-inline module, and folds changed-value contradictions through a uniquely delegating public parent that directly returns the helper call (#4681).

- Refuse child-helper reach from uncalled closures, async bodies, nested functions, compile-time item/const-block initializers, type/const-generic arguments, array-repeat lengths and unsupported macro token trees; retain parsed eager qualified delegation. (#4681)
