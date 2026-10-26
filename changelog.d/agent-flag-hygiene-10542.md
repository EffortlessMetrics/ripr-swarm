<!-- section: Fixed -->
- `ripr agent` value flags (`verify --before/--after/--gap-id`, `card`,
  `start`, `receipt` and `repair --seam-id/--phase`, `packet`) now refuse a
  repeated flag (`agent <command> takes <flag> once`) instead of silently
  keeping the last value, `agent verify --gap-id ""` is refused like
  `repair` and `packet`, `agent status --out` accepts `./target/ripr/workflow`
  and a trailing slash spelling of the default directory, and the
  `--max-seams`, `--cancel-after-ms` and `--at` errors no longer leak std
  integer-parse text (#10542).
