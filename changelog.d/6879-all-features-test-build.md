<!-- section: Fixed -->
- `cargo test -p ripr --lib --all-features` builds again: five Perl-gated
  test calls now pass the root path that a signature change added (#6879).
  The required Rust gate gains `cargo check -p ripr --all-targets
  --all-features`, so feature-gated test modules are compiled before merge
  and two separately green PRs can no longer break them on main (#6854).
