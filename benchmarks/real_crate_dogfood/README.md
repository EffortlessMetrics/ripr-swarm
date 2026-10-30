# Real-crate dogfood

Pinned recent commits of popular Rust crates, analyzed with `ripr check --base <revision>^`.
The manifest lists each crate, URL and full commit id; `note` names the defect a case
exposed (issue numbers) or marks a control. No upstream source is vendored.

```bash
cargo build -p ripr --bin ripr                      # or any binary to compare
cargo xtask real-crate-dogfood check                # offline manifest validation
cargo xtask real-crate-dogfood fetch --allow-network
cargo xtask real-crate-dogfood run --ripr target/debug/ripr
```

`run` writes `target/ripr/reports/real-crate-dogfood.{json,md}`: per case the wall time,
probe and finding counts by class, findings on deleted lines, related-test rows, tests listed
more than once, rows whose assertion was not credited, and findings that name a test in a file
the commit itself changed. Keep the JSON from a run before a change and compare it to a run
after (`--case <id>` limits the run). The counts are not correctness verdicts: judge a changed
number by reading the case's `ripr check --base <revision>^ --format human-full` output.
