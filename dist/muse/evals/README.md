# RIPR plugin evaluation contract

This is the minimum measurement contract for the Muse RIPR plugin.
It proves three separate claims; never merge their evidence.

| Layer | Claim | Evidence |
| --- | --- | --- |
| Plugin compatibility | Muse can validate, install, approve, discover, and invoke RIPR | Cold-session receipt (below) |
| Repair workflow | An agent can act on one genuine finding and produce a valid before/after receipt | Repair receipt (below) |
| Product effectiveness | The integration helps an agent write better discriminating tests at acceptable time, memory, and token cost | With/without comparison on pinned tasks; later, not claimed by the first two layers |

Static RIPR verdicts and executed-test verdicts are separate axes in
every layer. A better static class is not proof that a test rejects the
wrong implementation.

## Fixtures (pinned)

Positive case: the existing MCP journey fixture,
`benchmarks/agentic/mcp-journey` in the ripr repository
(`ripr-agentic-bench-manifest-v1`, fixture hashes in its
`manifest.json`). `input/base` holds the open predicate (`>`);
`input/journey` closes it (`>=`). The boundary test that asserts the
exact threshold value is the discriminating repair.

Executable oracle for the MCP layer (already in the repository):

```sh
cargo test -p ripr --test agentic_bench_mcp
```

Incorrect-repair control: the same fixture, but the added test asserts
only a non-boundary value (for example a price strictly inside the
range). Expected result: the executed test passes on both behaviors,
the wrong-implementation control in the workflow exposes it as
non-discriminating, and the repair must be reported as not completed
even if a static read looks better. A run that records this control as
an improvement is a false improvement and fails the evaluation.

Negative authority controls stay with the fixture's B3 probes
(`b3a.probes.ndjson`): typed failures only, no writes outside the
refresh cache, every frame within the advertised 131072-byte bound.

## Cold installation receipt (required fields)

Run in a fresh Muse session with no pre-registered RIPR MCP
server and no project `.mcp.json` entry for RIPR:

- Muse version (`muse --version`) and plugin version.
- `muse plugins validate <plugin-dir> --json`: `valid: true`, empty
  diagnostics.
- Install and approval: the exact commands used, and the runtime
  capabilities approved (skill, both commands, the `ripr` MCP server).
- Tool discovery: the ten `ripr_*` tools listed from the installed
  plugin's server, not from a hand-written host configuration.
- First calls: `ripr_workspace_status` and one `ripr_refresh` against a
  named repository, with outcome kind and snapshot id.
- Failures and refusals, verbatim typed codes. Hidden configuration
  (hand-edited MCP entries, pre-approved capabilities) invalidates the
  receipt.

## Repair receipt (required fields)

- Fixture identity: manifest schema version and fixture file hashes.
- Before: snapshot id, `canonical_id`, exposure class, readiness.
- Edit: the one test file changed and the assertion added.
- Executed: test command, exit status, and the wrong-implementation
  control result (must fail against the broken or pre-change behavior).
- After: new snapshot id, exposure class for the same item.
- Cost: elapsed wall time, tool-call count, approximate token use when
  the host reports it, and peak memory when measurable. Record `not
  measured` instead of estimating.
- Verdicts stated separately: static verdict, executed verdict, and the
  combined claim (completed repair only when both support it).

## Scoreboard connection

Report plugin results beside `dx-scoreboard` outputs; do not edit its
baselines or ceilings from a plugin run. Comparable rows must preserve
runner class, Muse version, plugin version, RIPR version, and
fixture identity. New baselines or thresholds need their own reviewed
change; a single successful run establishes existence, not a target.

## Expansion rule

Grow the corpus (more fixtures, with/without effectiveness cohorts)
only after the positive case and the incorrect-repair control above
have been reproduced end to end at least twice with retained receipts.
