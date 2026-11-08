<!-- section: Fixed -->
- Rust analysis keeps the public-parent to private-helper proof path when a helper moves into an inline, file-backed or parent-inline module, and folds changed-value contradictions through a uniquely delegating public parent that directly returns the helper call (#4681).

- Refuse child-helper reach from uncalled closures, async bodies, nested functions, compile-time item/const-block initializers, type/const-generic arguments, array-repeat lengths and unsupported macro token trees; retain parsed eager qualified delegation. (#4681)

- Require build-independent enabled `cfg`/`cfg_attr` ancestry for qualified child-helper reach, preserving constant-enabled calls and separate eager siblings without assuming a library was compiled with `cfg(test)`; refuse source-replacing proc attributes through the shared preservation authority. (#4681)

- Keep exact indexed owner identity for same-named ordinary helper delegation, and require an unambiguous production parent plus package-wide bare-name uniqueness or a same-module production witness. Relative module spellings, a crate-root filename and a truncated nested module cannot credit a different owner; supported real targets retain their witness. (#4681)
