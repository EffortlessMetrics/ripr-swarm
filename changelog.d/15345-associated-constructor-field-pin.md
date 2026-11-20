<!-- section: Fixed -->
- A constructed field checked by a whole-value `assert_eq!` against a
  derived-`PartialEq` literal now reads `exposed` when the owner is an
  associated constructor (`assert_eq!(Cfg::new(), Cfg { retries: 3, .. })`),
  as the same line's return value already did. The test must import the
  owner's type directly, as for that return value (#15345).
  A literal that neither version computes for the call's arguments stays
  `weakly_exposed` through `T::name(..)` as it does for a free function.
