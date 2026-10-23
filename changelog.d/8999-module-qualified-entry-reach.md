<!-- section: Fixed -->
- An integration test that calls a library entry through its module path (`my_crate::inner::entry(..)`, no `use`) is now related to the entry's changed private helper, like the imported spelling already was; module paths that are not the entry's exact path, packages with a `#[path]` remap and modules a binary also declares stay unrelated (#8999).
