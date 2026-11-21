<!-- section: Added -->
- Rust: a changed `let` binding that normalizes a bounded `find`, `rfind`
  or `len_utf8` result through `map_or` before a same-owner equality
  predicate now carries the named static limitation
  `rust_value_propagation_unresolved` in JSON and LSP gap output; the
  finding stays `static_unknown` (#3271).
- Library: the new `ripr::provider_contract` module exposes versioned,
  exact-snapshot provider DTOs for external proof orchestrators
  (`RiprAnalysisRequestV1`, `RiprAnalysisReceiptV1`, provider capability
  sets and contract errors) with `validate()` checks. The contract carries
  RIPR's static evidence only; it does not run tests, mutation tools,
  builds or edits (#3307, #3298).
- `ripr rerun --json` now discloses
  `cache.input_fingerprint.graph_provenance.path_dependency_graph`: a
  `complete`, `limited` or `unavailable` status, a detail, edge counts and
  per-manifest `forward` and `reverse` path-dependency neighbor lists built
  from local Cargo manifests without network access. It is disclosure only
  and does not change parity or `input_changed` decisions (#3613).
- LSP: when `ripr.toml` registers test harnesses, the editor workspace status
  now carries a `harness_registry` summary with one row per registration
  (registration id, harness kind, adapter, target, subject count and
  limitation count), or `available: false` with a reason when the run could
  not establish those facts (for example a limited run). The key is omitted
  for repositories without registrations. These are registered facts, not a
  claim that the harness subjects ran (#3622, #3605).
- Rust: a test that matches directly on a call to the changed function
  (`match parse(..) { Ok(..) => .., Err(E::Variant) => .. }`, or an
  `Err(e) if e == E::Variant` arm with a failing catch-all) and fails loudly
  in every `Err` arm is now read as a `guarded_result_match` oracle. An
  exact error variant in the pattern or guard is credited as strong and a
  concrete `downcast::<T>` pin as medium. Wildcard or swallowed `Err` arms,
  message-only checks, a method-receiver, chained or `?` scrutinee, and a
  shadowed callee are not credited. `guarded_result_match` joins the JSON
  oracle kinds (#3731, #3709).
- `ripr agent repair --phase before` accepts
  `--python-repair-trust-manifest` and `--python-repair-trust-attempt`, with
  the `--edit-authorized` / `--edit-authority` pair, to bind a repair
  attempt to one accepted Python repair-trust selection row by digest. The
  binding is checked before the attempt is published, kept with the attempt
  and mirrored at `target/ripr/workflow/python-repair-trust-binding.json`, and
  `--phase after` checks it again before recording the applied edit
  (#3737, #3569).
- `ripr review-comments` cards (`llm_guidance.repair_command`), `ripr gate
  evaluate` repair routes and inline PR comments now lead with
  `ripr agent repair --root . --seam-id <id> --phase before` when the named
  seam is actionable, passes the repair-packet check and has a recommended
  test the repair can edit. Other items carry no repair command and keep the
  existing verify guidance (#3936, #3906).
- `ripr first-pr` now falls back to the review cards when the gap ledger
  selects no top gap: it picks the first actionable card that carries
  `llm_guidance.repair_command` for the same root, base and head, shows a
  `Start repair` line in `start-here.md`, and records the command unchanged
  as `selected.repair_command`. `ripr pr-summary` carries it as
  `top_repair.repair_command` and as the first local reproduction command.
  When no card supplies one, the no-action reason says why (#3960, #3906).
- First-useful-action, the PR-review front panel, the PR evidence ledger
  and RIPR Zero status now lead with the `ripr agent repair --seam-id <id>
  --phase before` start when an input artifact already carries it as
  `repair_command`, so a fresh PR is no longer asked for post-repair
  assistant proof before any repair exists. Without a carried start they
  keep the verify guidance (#3962, #3906).
- `ripr pilot` JSON gains `language_routes`: when the workspace also holds
  TypeScript, JavaScript, Python or Perl files it names each language,
  its file count and status, and the `ripr check --root <root>` route (or
  that the adapter is not in this binary). With no Rust seams the state
  is `required`, so an empty ranking is no longer read as a clean pass
  for those languages (#3963).
- A changed public function declared in a package's library root file (such
  as `src/lib.rs`) with no existing test that can safely take the repair can
  now get one producer-owned Integration new-test proposal: a new file under
  the package's `tests/` directory, offered only when the package keeps
  Cargo's `tests/` autodiscovery and already has a `.rs` file there. Private
  functions, functions inside modules, `autotests = false` packages and
  packages without a library target still get no proposal (#4625, #4576).
- Added the `@effortlessmetrics/ripr` npm launcher package
  (`packaging/npm/launcher`). Its `ripr` command selects the exact-version
  native package for the current platform (Linux glibc x64/ARM64, macOS
  x64/ARM64, Windows x64 MSVC) and runs it, passing through the child's exit
  status. It runs no install scripts, downloads nothing, invokes no shell and
  never falls back to another `ripr` on `PATH`; it requires Node 20 or later
  (#4819, #4776).
- `ripr agent repair` (every phase) and `ripr agent status` accept
  `--store PATH`, an explicit repair-attempt store resolved against
  `--root` (default `target/ripr/repair-attempts`). A missing explicit store
  does not fall back to the default, and a store path that leaves the root,
  contains `..`, or resolves through an alias or UNC path is refused
  (#4844).
- `ripr help --json` prints one versioned JSON document for machine
  discovery: the command catalog, per-command metadata and workflow steps
  as sorted rows, a `sha256` digest over the catalog, and explicit
  limitations. The output is the same across equivalent roots and terminal,
  color, width and locale settings, any extra argument is a usage error, and
  a catalog integrity failure writes nothing to stdout (#4971, #4825).
- `[languages.rust] handwritten_files` in `ripr.toml` lists exact
  repository-relative `.rs` paths to analyze even though their filename or a
  `gen/`, `generated/` or `out/` directory matches a built-in generated-code
  convention. Explicit `generated_file_patterns`, generator headers and
  `cargo vendor` markers still exclude a file. The setting applies to diff
  checks and the repository seam inventory and counts as finding-affecting
  configuration when artifacts and receipts are compared (#5025).
