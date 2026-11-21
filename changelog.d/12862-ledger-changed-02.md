<!-- section: Changed -->
- Rust checks: a finding's propagation evidence reads "Complete
  propagation witness reaches ..." only when an owner-bound path from the
  changed expression to the returned value, error variant or field is
  established. An assertion that pins a sibling error variant is no longer
  credited as confirming a changed `Err(..)` construction on return-value
  or field-construction probes, so such a finding stays `weakly_exposed`
  (#3547).
- Rust source-role analysis now carries a test-only context across files: a
  function in a file reached through an out-of-line `#[cfg(test)] mod
  tests;` chain, or in a file-level `include!` fragment under such a module,
  reads as test evidence instead of seeding production findings. A
  non-literal `#[path]`, an ambiguous module parent, a cycle or conflicting
  contexts fail closed (#3592).
- Rust checks in Draft and Fast modes that include unchanged tests now widen
  the changed-package scope to workspace packages that reach a changed
  package through local `path` dependencies, so tests in a crate that
  depends on the changed crate are considered. When the path-dependency
  graph is limited, a stderr notice says dependents may have been missed;
  when it is unavailable, the notice says the scope stays at the changed
  packages (#3616, #2970).
- Rust: when an owner's name is defined in more than one workspace crate, a
  test in another crate that calls it is now related to the owner if a
  direct path dependency joins the two packages, in either direction, and
  the workspace index and dependency graph are complete. A same-named
  definition in the test's own package, a competing same-named candidate
  reachable through an edge, or a limited or unavailable graph still leaves
  the test unrelated (#3619, #2972).
- Rust match-arm analysis also credits a changed tuple arm over two
  immutable `bool` locals derived inside one closure (for example in a
  `filter_map`), when a test calls the owner and asserts equality on the
  exact result for that input pair; such an arm can read `exposed` instead of
  `weakly_exposed`. Unsupported owner, closure, initializer, result-flow and
  oracle shapes stay unverified (#3769).
- A failing `ripr doctor` now ends by saying each `!` line above names the
  check and its fix, instead of pointing at `ripr doctor --help`, which is
  not the remedy for any failed check (#3902).
- `ripr agent status` now resumes a repair attempt: `next_command` is the
  awaiting attempt's `--phase after` command, otherwise a new attempt for
  the one seam whose attempts ended without closing its gap, otherwise the
  first missing artifact, with `ripr pilot` when no seam is known yet. It
  lists every attempt under `repair_attempts[]` with a disposition, and
  when it would have to guess (unreadable manifest, several awaiting
  attempts or open seams, unreadable `HEAD`) it leaves `next_command` null
  and a warning names the choices (#3931).
- `ripr pilot` now closes with the `ripr agent repair --seam-id <id>
  --phase before` route when the top seam passes the repair-packet
  eligibility check: the terminal ends with the numbered repair steps, the
  Markdown Next Commands block offers that command, and `pilot-summary.json`
  carries `next.repair_command` (null otherwise). A seam without a repair
  route keeps the `ripr check` snapshot plus `ripr outcome` comparison
  (#3932).
- PR review surfaces lead with the repair start when one is carried:
  review cards, gate repair routes and inline comments carry
  `ripr agent repair --root . --seam-id <id> --phase before` for a seam that
  passes repair-packet eligibility, and `first-action`,
  `pr-review front-panel`, the PR evidence ledger, `first-pr` and
  `pr-summary` show that carried command first. They no longer build an
  `agent start --seam-id` command from a bare seam or gap id, and a seam
  whose recommended test `ripr agent repair` cannot edit gets no repair
  start (#3987, #3906).
- The workflow `ripr init --ci github` generates now cancels a run still in
  progress for the same pull request when a newer push arrives, so two
  overlapping runs no longer both post the same comments (#4474).
- `ripr pilot` and `ripr first-pr` now name the output directory they could
  not create, keep the OS error, and on a permission-denied or read-only
  filesystem error add which flag writes elsewhere (`--out PATH` for pilot,
  the output-directory flag for first-pr). Exit status stays `2` (#4774).
- `ripr mcp --stdio` now runs on the pinned official Rust MCP SDK (`rmcp`),
  which owns protocol negotiation, dispatch, request correlation and
  cancellation, behind ripr's bounded stdio transport (256 KiB input and
  128 KiB response caps). Clients must await replies before closing stdin:
  EOF ends the session. ADR 0022 records MCP as a bounded, read-only
  projection over shared ripr authority (#4899).
- `ripr check --format human-full` now prints an `id:` line in each finding
  block, the same finding id that JSON output, `ripr explain` and
  `ripr context` use, so the full form names each finding (#4924, #4321).
- `ripr check --format agent-seam-packets-json` (schema `0.5`): each seam
  packet now states the edit cage its repair enforces, with
  `allowed_edit_surface` (the selected test file), `forbidden_files` (the
  changed production file) and `must_not_change`, including that an edit
  outside the surface ends the attempt. `ripr agent receipt` accepts
  `--attempt <id>` to bind to one attempt's retained packet, and repair-loop
  refusals name the start command when no attempt matches and list the
  attempt ids when several do (#4935, #4330, #4332).
- Default `ripr check` human output now states its display caps: the
  related-test and observed-value lists say how many entries they show out
  of how many and point to `--format json` for the rest, the digest's
  `Related test` and `Missing check` lines read `(1 of N)`, and the
  `Hidden:` block names each omitted finding by `file:line` and class, up
  to 20 entries. It no longer says findings were suppressed by policy when
  none were. The caps and the JSON and SARIF output are unchanged (#4320,
  #4942).
