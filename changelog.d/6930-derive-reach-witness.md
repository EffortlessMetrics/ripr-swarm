<!-- section: Fixed -->
- Attribute arguments such as `#[derive(Error)]` are no longer read as calls to
  a function named `derive`. A test file that applies a derive whose
  `#[proc_macro_derive]` function reaches the changed code now gets a witness
  naming the derive and that function ("applies `#[derive(Error)]`, expanded by
  `derive_error`") instead of a spurious path through `derive`. The verdict
  stays `no_static_path` with the named transitive-reach limitation; ripr does
  not expand the macro (#6924, #6930).
