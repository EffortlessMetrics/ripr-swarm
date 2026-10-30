<!-- section: Changed -->
- A same-file `macro_rules!` test table, such as itoa's `test! { name(value, expected) ... }`, now indexes one test per row. Before, the code those tests call read as `no_static_path` even when a row pinned the changed boundary. Row layouts the expander cannot split exactly (`=>` rows, a trailing separator, a leftover token) still index nothing (#6925).
