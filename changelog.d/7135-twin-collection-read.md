<!-- section: Fixed -->
- Rust: a field-collection comparison between two bindings that each ran the owner (`c.add(20); d.add(20); assert_eq!(c.items, d.items)`) no longer confirms the changed push, since a wrong owner moves both sides together (#7135).
