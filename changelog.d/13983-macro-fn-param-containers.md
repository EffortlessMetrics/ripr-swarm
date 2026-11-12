<!-- section: Fixed -->
- `ripr check` no longer probes fn parameter lines as `field_construction` when the fn sits in a macro-wrapped `impl`, `trait` or `mod` body or a nested item macro, or when its generic list holds `Item = u8` or `Fn() -> u8` bounds; a fn nested in another fn body inside a macro, and call arguments, keep their probes (#13983).
