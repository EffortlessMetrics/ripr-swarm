<!-- section: Fixed -->
- A TypeScript AVA test that pins the thrown message with `t.throws(fn, { message: "..." })` or `t.throwsAsync(...)` now reads as an exact error check, like `node:assert`'s object form, instead of `broad_error`; a change that no message separates stays weak (#10400).
- A `node:assert` `throws` / `rejects` expectation object with a duplicate `message` key now reads as `broad_error` instead of `exact_error_variant`; exact credit needs exactly one string-literal `message` (#10400).
