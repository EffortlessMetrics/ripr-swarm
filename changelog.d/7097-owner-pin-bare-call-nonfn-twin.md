<!-- section: Fixed -->
- An owner-return pin no longer credits a bare call `f(..)` when the test's
  binding of the name resolves elsewhere: a same-named `static`, `const`,
  tuple struct, extern item or competing `use` in the test's body or own
  module scope takes the call. An explicit `use` must be the only one
  binding the name in its scope and resolve to a module holding the owner
  (directly or through one re-export) with no twin beside it; a glob must
  deliver the owner from such a twin-free module; with neither, the test
  must sit in the owner's own module. Anything unplaced refuses (#7097).
- Manifest authority no longer treats an explicit `[lib] path = "src/lib.rs"`
  as a moved library root: spelling the default out keeps the owner's own
  package naming its library (#7097).
