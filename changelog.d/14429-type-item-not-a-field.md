<!-- section: Fixed -->
- Rust checks: a changed `type` item such as `type Eq: CmpEq;` is no longer reported as a constructed field with "Missing check value: type Eq: CmpEq;"; it is one unclear (`static_unknown`) declaration finding (#14429).
