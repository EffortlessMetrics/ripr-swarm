<!-- section: Fixed -->
- Verdicts: a one-line selector swap (`min_by_key` to `max_by_key`, `first` to
  `last`, `next` to `next_back`) on a line ending in `?` no longer reads as an
  `error_path` gap with "execute the changed error arm" guidance. The `?` only
  propagates the empty case, so the line abstains as `static_unknown`. A changed
  `?` error variant, an added `?`, and any other edit keep their `error_path`
  family (#9866).
