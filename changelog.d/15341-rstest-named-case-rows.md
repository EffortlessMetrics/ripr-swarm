<!-- section: Fixed -->
- rstest's named case rows (`#[case::below(50, 0)]`) are now read as case inputs like `#[case(50, 0)]`, so a boundary row written that way counts as a test input instead of leaving the finding unknown (#15341).
