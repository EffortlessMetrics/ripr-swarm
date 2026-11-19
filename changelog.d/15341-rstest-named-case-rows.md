<!-- section: Fixed -->
- rstest's named case rows (`#[case::below(50, 0)]`) are now read as case inputs like `#[case(50, 0)]`, so a boundary row written that way is credited and a missing one is named instead of reading as unknown (#15341).
