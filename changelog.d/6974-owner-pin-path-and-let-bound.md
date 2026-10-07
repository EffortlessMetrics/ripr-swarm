<!-- section: Fixed -->
- An exact `assert_eq!` on a free function called through a path to the
  function's own module (`super::f(..)` or `crate::f(..)` in a unit test,
  `my_crate::f(..)` from an integration test) or through a once-bound
  result (`let r = f(..); assert_eq!(r, 7);`) now confirms the owner's
  returned value, as the imported bare call `f(..)` already did. These read
  `weakly_exposed` before, a false actionable gap. A path to any other
  module (a re-export, another crate), a raw-identifier or macro shadow of
  the name, and a binding that is mutable, rebound, borrowed or used outside
  its assertions stay unpinned (#6974).
