<!-- section: Fixed -->
- Snapshot owner-binding scope tracking blanks `br".."` and `cr".."` raw strings and nested block comments before reading bindings, so a raw string's backslash no longer swallows a later shadowing `let` and keeps owner credit.
