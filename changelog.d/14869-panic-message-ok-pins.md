<!-- section: Fixed -->
- On an error variant change, related tests with a panic expectation retain
  their original miss reason because the panic message can name the variant.
  A direct `matches!(f(), Ok(_))` success check is marked as unable to
  distinguish the variants. A recorded success-only `debug_assert_eq!` receives the same reason as its
  `assert_eq!` counterpart (#14869).
