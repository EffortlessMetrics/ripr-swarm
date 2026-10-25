<!-- section: Fixed -->
- Computed boundary operands retain integer parameter type limits instead of borrowing `usize` overflow semantics, refuse shadowed intrinsic type names in both direct expressions and local initializers, and check `map_or`'s eager default on both option arms before crediting an exact value (#3295).
