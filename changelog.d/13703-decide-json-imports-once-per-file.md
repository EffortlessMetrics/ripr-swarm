<!-- section: Changed -->
- Rust checks: the assertion escape scan decides which `use` items are canonical `json!` imports once per file instead of once per macro call and `use` item; findings are unchanged (#13703).
