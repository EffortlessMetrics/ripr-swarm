<!-- section: Fixed -->
- A TypeScript AVA test that pins the thrown message with `t.throws(fn, { message: "..." })` or `t.throwsAsync(...)` now reads as an exact error check, like `node:assert`'s object form, instead of `broad_error`; a change that no message separates stays weak (#10400).
