<!-- section: Fixed -->
- The per-thread Rust parse memo now retains files up to 2 MiB (was 512 KiB),
  so a large analyzer file such as `owner_pin.rs` is parsed once per distinct
  source instead of on every `parse_clean_source_file` call (#8629).
