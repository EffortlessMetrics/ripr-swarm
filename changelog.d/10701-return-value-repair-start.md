<!-- section: Fixed -->
- `ripr repair` now starts from a weak test on a changed return value or error
  when a direct caller test with an editable target exists and reach, activation
  and propagation are all established; tests that reach the function only as a
  function value or inside an executor helper such as `block_on` are left alone.
  When no safe start exists, the refusal names the manual `ripr agent stub`
  route (#10701).
  The start is withheld, with the same plain reason, for a related `#[ignore]`
  or `#[should_panic]` test, a changed struct literal, a returned type that may
  have a non-plain `Drop` (followed through `Self`, `use .. as` and
  `use Type::{self as Name}` renames, and the fields of every workspace struct,
  enum or union of that name; an unresolved type alias, a return type naming a
  generic parameter of the function or of a preceding `impl` or `trait`
  header, `Self` in a generic `impl`, an `impl Trait` or `dyn Trait` return,
  or a workspace holding a macro, `extern crate` or outside-crate item also
  withholds), a test that compares the owner call, or
  a local bound from it through `let` or a tuple destructure, with `==`,
  `.eq(`, `!(.. != ..)` or a closed `>= .. && .. <=` range, and an exact
  error-variant assertion naming a different variant. A bool local
  (`let ok = v == 6; assert!(ok)`), an open range (`v > 5 && v < 7`) and
  `(6..=6).contains(&v)` are not recognized as pins and still start; a
  lowercase type name is not followed for `Drop`. A trait default method
  with `where Self: Sized` also withholds (an over-withhold).
  `ripr repair <seam>` does not apply the finding-level static-limited verdict
  that `ripr check` uses; it relies on the seam-level evidence and these gates.
  An assertion observes the owner through a bare or method call of its name,
  or a path-qualified call whose qualifier resolves to the owner through the
  shared call identity (`crate`/`self`/`super`, the owner's crate name, a
  `use`-bound path, its module path). A same-name call through another module
  (`other::dbl(2)`), another type or primitive (`X::dbl(2)`, `u32::dbl(2)`), an
  uppercase module, or a qualifier that cannot be read, does not observe it; a
  method is observed only through a method call or its own self type (by
  type name, so same-named types in different modules are not told apart;
  #10948), and a later `let` rebinding of a bound name drops it.
