<!-- section: Fixed -->
- A test that calls an operator impl through its type (`Money::add(a, b)` or `<Money as Add>::add(a, b)`) is now judged by its own evidence instead of being treated as a syntax-only credit; another type's `add`, a longer type or method name and module-qualified paths stay syntax-only (RIPR-SPEC-0240, #10551).
