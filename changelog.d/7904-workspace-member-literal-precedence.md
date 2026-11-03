<!-- section: Fixed -->
- Rust workspace macro ownership keeps an explicitly listed `[workspace]
  members` path even when the same path is listed in `exclude`; exclusions
  still apply to glob-expanded members (#7904).
