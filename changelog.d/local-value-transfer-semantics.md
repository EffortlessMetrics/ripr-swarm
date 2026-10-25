<!-- section: Fixed -->
- Computed local boundary operands retain integer parameter type limits instead of borrowing `usize` overflow semantics, refuse shadowed intrinsic type names, and check `map_or`'s eager default on both option arms before crediting an exact value (#3295).
