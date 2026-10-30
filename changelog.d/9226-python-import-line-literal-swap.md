<!-- section: Changed -->
- A Python edit that changes only integer-literal values on a line that calls an imported symbol (`islice(seq, 0, ...)` to `islice(seq, 1, ...)`) is no longer `static_unknown` with `missing_import_graph`: it reads as `weakly_exposed` with no repair card, never `exposed`, because the callee is not evaluated (#9226).
