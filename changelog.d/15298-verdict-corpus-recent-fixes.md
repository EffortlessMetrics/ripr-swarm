<!-- section: Internal / CI -->
- Verdict corpus: add six authored cases pinning four recently fixed wrong results, so they cannot come back unnoticed: a let-bound result also named in message arguments (#14158), a `std::debug_assert_eq!` pin (#8637), a raw-identifier `use` leaf shadowing a glob namesake (#14087), and a bool owner with a string-equality tail (#15030) (#15298).
