<!-- section: Fixed -->
- A helper that calls `module::owner()` where `module` is the helper's own generic type parameter (`fn route<helpers: Trait>() { helpers::owner() }`) no longer counts as a call to the same-spelled module's `owner`, so repo-exposure stops crediting a function the test never runs (#12891).
