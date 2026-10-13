<!-- section: Fixed -->
- Rust: a changed match arm over an enum with single-letter or all-caps
  variants (`Kind::A`) now names the missing discriminator, such as `_` or
  `Kind::B`, when no related test selects the arm. Such a name may be an
  associated constant, so ripr reads it as a variant only when the path is
  `Kind::A` (or `Self::A` in `Kind`'s own impl), the scrutinee's type is
  written as plain `Kind`, and the owner's file declares the one `enum Kind`
  listing `A` with no import, module glob (an inline test module's
  `use super::*` is fine), item macro or other item that could bind `Kind`.
  A test's `Kind::A` input is read only when no related test file declares
  its own `Kind` or glob-imports another module. Otherwise the arm stays
  unknown, as before
  ([#7543](https://github.com/EffortlessMetrics/ripr-swarm/issues/7543)).
