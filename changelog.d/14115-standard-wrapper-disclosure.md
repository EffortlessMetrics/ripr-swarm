<!-- section: Fixed -->
- The "assertion not credited" line no longer blames a standard `assert!`
  whose arguments nest `assert_eq!` (or use a variable named `macro_use`) in
  an unrelated file; it names the import or definition that actually blocks
  credit (#14115).
