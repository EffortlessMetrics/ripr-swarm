# ripr Repository Metadata

This directory holds lightweight repository metadata used by humans, agents, and
xtask policy checks.

Files:

- `traceability.toml`: maps specs to tests, code modules, output contracts, and
  metrics; validate it with `cargo xtask check-traceability`.
- `goals/campaigns/*.toml`: durable campaign records containing objectives,
  work-item identities, dependencies, stackability, and acceptance commands.
- `goals/active.toml`: a non-authoritative compatibility pointer retained during
  the multi-campaign migration.
- `no-panic-allowlist.txt`: tracks existing panic-family debt by path, pattern,
  and maximum count.
- `static-language-allowlist.txt`: lists files that may mention prohibited
  mutation-runtime terms because they define the language boundary or discuss
  calibration.

The preferred direction is to remove allowlist entries as implementation and
test debt is paid down. New entries should be reviewed as deliberate exceptions.
