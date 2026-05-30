pub(super) const SWARM_HELP: &str = r#"Queue bounded repair work for external agent coordination.

Usage: ripr swarm <subcommand>

Subcommands:
  queue  Rank GapRecord-backed agent packets into a bounded repair queue.

Run `ripr swarm queue --help` for the queue surface. Swarm commands are
advisory and static; they do not run tests, call providers, generate tests,
edit files, run mutation testing, create receipts, or change gate behavior.
"#;

pub(super) const SWARM_QUEUE_HELP: &str = r#"Queue GapRecord-backed repair packets for safe agent assignment.

Usage: ripr swarm queue [--root PATH] [--gap-ledger PATH] [--language python] [--top N] [--format json]

Options:
  --root PATH        Workspace root. Defaults to current directory.
  --gap-ledger PATH  Gap decision ledger JSON. Defaults to target/ripr/reports/gap-decision-ledger.json.
  --language NAME    Language filter. Defaults to python.
  --top N            Maximum queued packets to return. Defaults to 10.
  --format json      Explicit JSON output. JSON is the only queue format.
  --json             Compatibility alias for JSON output.

The queue command reads existing GapRecord artifacts and includes only records
that are already eligible for `ripr agent packet --gap-ledger ... --gap-id ...`.
It groups packets by `allowed_edit_surface` conflict group so schedulers can
avoid parallel edits to the same test file. Staleness is reported as
`not_evaluated` until a later receipt/ledger step compares the queue with the
current git state.
"#;
