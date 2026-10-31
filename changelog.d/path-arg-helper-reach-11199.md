<!-- section: Fixed -->
- A test that hands an inherent `Type::method` path to a same-file helper that
  unconditionally runs that parameter (`case(Pricer::discount, 150, 140)` with
  `fn case(f: fn(&Pricer, u32) -> u32, ..) { assert_eq!(f(&Pricer, a), w); }`)
  is now related to the changed method with `helper_owner_call` instead of a
  name-only or file-proximity relation, so the finding reports `reach yes`
  rather than "no test is seen calling" the method. The relation grants reach
  only, never an oracle or `direct_owner_call`. It needs the owner's `impl` to
  be a root item of the test's file, the type name unrebound there, no other
  definition `Type::method` could name, and a helper that is the one
  workspace-unique same-file free function and runs the parameter on every
  call (not under a branch, loop, closure, `async`, or the right side of
  `&&`/`||`). A path to another type's same-named method, a trait method, an
  unresolvable or qualified type head, a stored or forwarded function, and a
  `bitflags!`-generated type stay unrelated (#11199).
