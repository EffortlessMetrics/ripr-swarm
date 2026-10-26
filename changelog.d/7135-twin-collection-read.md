<!-- section: Fixed -->
- Rust: a field-collection comparison between two bindings with equivalent owner histories (same construction and same ordered driving statements, `c.add(20); d.add(20); assert_eq!(c.items, d.items)`) no longer confirms the changed push, since a wrong owner moves both sides together. Differing arguments, call counts or seeding keep the credit (#7135).
