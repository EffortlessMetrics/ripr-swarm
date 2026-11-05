<!-- section: Fixed -->
- Rust analysis keeps the public-parent to private-helper proof path when a helper moves into an inline, file-backed or parent-inline module, and folds changed-value contradictions through a uniquely delegating public parent that directly returns the helper call (#4681).

- Refuse child-helper reach from uncalled closures, async bodies, nested functions, compile-time item/const-block initializers, type/const-generic arguments, array-repeat lengths and unsupported macro token trees; retain parsed eager qualified delegation. (#4681)

- Require build-independent enabled `cfg`/`cfg_attr` ancestry for qualified child-helper reach, preserving constant-enabled calls and separate eager siblings without assuming a library was compiled with `cfg(test)`; refuse source-replacing proc attributes through the shared preservation authority. (#4681)

- Preserve the resolved inline module in delegation keys when another package owner has the same helper name, and resolve ordinary inline owners through their indexed module in the consumer, so an inline rival cannot credit an unrelated file-backed child while the real target keeps its witness. (#4681)
