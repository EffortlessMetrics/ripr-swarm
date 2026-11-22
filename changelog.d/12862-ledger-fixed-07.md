<!-- section: Fixed -->
- `ripr pr-ledger record` now reads a receipt written by `ripr receipt
  write` as `receipt_found` instead of not recognizing it. An absent,
  incomplete or malformed receipt still does not count as found (#6611).
- Python preview: an owner decorated with `lru_cache`, `cache` or
  `cached_property` bound from `functools` no longer carries the
  `decorator_indirection` limit, so a test that asserts on its result can be
  credited. `@retry` and a local decorator of the same name still keep the
  limit (#6728, #6652).
- `ripr first-pr` preflight now names each unresolvable ref once with its
  role (`Could not resolve --base ...` or `--head ...`), instead of two
  identical warnings naming neither, and prints its root, manifest, config
  and output paths in the same slash spelling as its next commands
  (#6862, #5252).
- `ripr check` no longer aborts with "byte index is not a char boundary"
  when a test calls through a path with a non-ASCII segment such as
  `crate::módulo::render`; the segment is now read as part of the name
  (#7073).
- A related test whose assertion contradicts the owner's current value no
  longer lets a consistent sibling test close the gap. Its call-site values
  no longer count as activation or boundary evidence, and while it remains
  the repair outcome reports the gap as improved rather than closed and
  names the contradicted test in the weak or unknown evidence section
  (#7122, #7007).
- TypeScript checks no longer report the `typescript_test_runner_unresolved`
  limitation when a runner such as vitest or mocha was detected but no
  verify command could be built, which contradicted the finding's
  `typescript_test_runner: <name>` fact. The limitation now means the
  runner is unknown (#8058, #7646).
- Rust: when a file binds its own `vec!`, `format!` or another trusted
  macro name, a trusted assertion macro named inside that macro's arguments
  is now treated as possibly redefined, so assertions written with it are
  no longer credited as an owner pin (#8345, #5941).
- Rust: a test that writes its own expected file from the owner's result
  and then compares the owner against what it reads back no longer credits
  the changed return as `exposed`; the finding reads `weakly_exposed`. A
  committed golden file that the test only reads keeps its credit
  (#8366, #8097).
- `ripr check` no longer creates probes or findings from prose inside a
  Rust block comment (`/* */`, `/*! */`, `/** */`); a line such as
  `For example:` inside one was read as a field construction (#8400).
- Windows: the `ripr receipt write` command and the gap-decision ledger's
  check-after command stored in JSON records now use POSIX single-quote
  quoting on every host instead of the host shell's quoting, so the stored
  string is the same across machines (#8501, #8437).
- `ripr mcp --stdio`: a request to a served method (such as `tools/call`,
  `resources/read` or `server/discover`) whose params fail typed
  validation now answers Invalid Params (`-32602`, "invalid params for
  `<method>`") instead of Method Not Found (`-32601`), so a client can tell
  a bad call from a missing method (#8532).
- A bare lowercase `match` binding in a file that invokes an item macro
  (such as `def_const!(target);`) now demotes binding-arm selection, as a
  file-root `use super::*` already does, because the macro may declare a
  constant of that name (#7863, #8580).
- `ripr check` in diff mode no longer reads a predicate or return-value
  finding as `exposed` from a related test whose assertions contradict each
  other at the changed boundary (for example `assert!(is_adult(18));`
  followed by `assert!(!is_adult(18));`). That test is not credited for
  activation, and the finding reads `infection_unknown` with a typed
  limitation, as the repo-exposure surface already did. A test with a
  consistent assertion keeps its credit (#11012, #10980).
- `ripr reports ci-packet` runs its PR-guidance `ripr check --format json`
  child with the findings-array byte budget disabled
  (`RIPR_CHECK_FINDINGS_BYTES=0`), so a pull request whose findings exceed
  the default 1 MB budget still hands `review-comments` a complete document
  instead of a findings-bounded one it must refuse. Other packet children
  and hand-run `ripr check` keep the ambient budget (#11154, #11143).
- With `ripr check --diff -`, the notice for a diff with no parseable file
  changes now says to verify the diff piped on stdin, instead of telling the
  user to check a `--diff` path that was never given (#11455).
