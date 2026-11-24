<!-- section: Fixed -->
- Windows: when Git cannot start because the workspace root is longer than
  258 characters, ripr now names the root length and says to move the
  repository to a shorter path, instead of a bare OS error. `ripr doctor`
  finds `.cmd` and `.bat` tool shims (such as an npm-installed `pnpm.cmd`)
  when no `.exe` is on `PATH`, the PowerShell evidence-capture command in
  PR summaries switches the console to UTF-8 while capturing, and the VS
  Code extension downloads the x64 server on Windows ARM64 instead of
  offering none (#4415).
- VS Code: in a multi-root window the session binds to the folder
  selected as the workspace root, so the server no longer reports the
  root as ambiguous after you pick one, and
  status text and notifications name `ripr first-pr --root .` and
  `ripr pr-summary` instead of `cargo xtask` commands. The language server
  offers the per-finding inspect and navigate actions as `quickfix.ripr.*`
  code actions, so they appear in the lightbulb and Quick Fix menu, and the
  hover, code lens and top limitation agree about the same finding (#4419).
- A `ripr doctor` tool probe that times out or cannot be waited on now has
  its whole process tree ended, and a probe whose cleanup cannot be
  confirmed says some of its processes may still be running. On Windows,
  `ripr agent verify-execute` keeps `USERPROFILE`, `HOMEDRIVE` and
  `HOMEPATH` in the child environment floor so Git reads the global
  config, and the long-workspace-root error now leads with "clone or move
  the repository to a shorter path" so editors that shorten the message
  keep the remedy (#4430).
- SARIF: `physicalLocation.artifactLocation.uri` is now percent-encoded as an
  RFC 3986 URI reference, so a path with spaces, `#`, `%`, commas, colons or
  non-ASCII characters validates and resolves to the right file.
  Fingerprints still hash the unencoded path (#4443).
- `ripr check --format github` now writes its disclosure notices before the
  per-finding annotations, and adds a notice naming how many annotations of
  a level were emitted when a level passes 10. GitHub Actions shows at most
  10 annotations per level in one step and drops the rest, so before, those
  disclosures could be among the dropped lines (#4451).
- The preview-language note ("N TypeScript files analyzed under preview
  support") no longer counts changed files the TypeScript or Python adapter
  skips, such as files under `vendor/`, `node_modules/`, `dist/`, `build/`,
  `.venv/` or `site-packages/` and generated files; its count and sample
  paths now match the files the adapter analyzes (#4472, #4372).
- LSP: a finding diagnostic on an indented line now starts at the changed
  expression on the saved line, measured in the negotiated position
  encoding, instead of at character 0, so it no longer covers the leading
  whitespace and hover on the changed expression shows the finding (#4688,
  #4602).
- Without `git` on `PATH`, `ripr check` now says git was not found on PATH
  and points at a saved diff (`--diff PATH` or `--diff -`) instead of
  printing the failed git command line, including when `--base` is omitted,
  and `ripr doctor` recommends the `--diff` route. The exit code stays `2`
  (#4735).
- Python: when a changed block also holds a behavioral line, ripr no longer
  reports separate findings for the bracket-only lines (`)`, `],`, `else:`)
  inside it, and an added import line that does not replace an import is no
  longer a probe of its own; the behavioral lines carry the change. A block
  made only of bracket-only lines is still analyzed (#4752, #4767).
- A changed Rust file the parser refused and read lexically now makes the
  `ripr check` outcome `partial_with_limitations` with a named limitation,
  so `ripr gate evaluate` exits `2` instead of passing on part of the diff.
  A gate baseline of an unrecognized shape is a `config_error` instead of
  an empty baseline, PR guidance must carry `tool: ripr` and a known
  status, a named ledger that cannot be read now exits `2`, and the
  generated workflow reruns on `labeled` and `unlabeled` events when a
  blocking `RIPR_GATE_MODE` is set, so adding or removing `ripr-waive`
  reruns the gate. `docs/EXIT_CODES.md` now matches these exits (#4773,
  #4722, #4728).
- TypeScript and JavaScript: a change inside a member-assigned function
  (`res.send = function () {...}`, `Foo.prototype.bar = () => ...`) or a
  function inside a top-level call (an IIFE, or a wrapper argument such as
  `define([...], function () {...})`) now carries a
  `typescript_owner_extraction_partial` limitation instead of producing no
  finding and no disclosure. A `@types/node` dependency beside a test runner
  no longer counts as a second runner signal, so such projects are no longer
  reported as having an ambiguous framework (#4779, #4754, #4762).
- Repository seam inventory and repo exposure (used by `ripr pilot` and
  `ripr agent`) now skip generated Rust the same way `ripr check` does:
  conventional names and directories such as `bindings.rs`, `*.gen.rs`,
  `generated/` and `out/`, plus `[languages.rust] generated_file_patterns`.
  Repo exposure discloses the skip as a `generated_rust_source_skipped`
  limitation with the skipped file count and up to three paths, and
  `run_status` stays `complete` (#4788).
- Rust: a test calling a method whose name is also defined on another type
  in the workspace is now related to the changed impl method as a direct
  owner call only when its receiver resolves to that impl; otherwise it
  is a weak name match. A test of itertools' `Combinations::size_hint` no
  longer makes a change to `WhileSome::size_hint` read `exposed`. Unique
  method names and owner-type constructors keep the direct relation, and
  past the eight-row `related_tests` cap distinct tests are listed first
  (#4885).
- LSP: when a saved Rust file changes again while a diagnostics refresh is
  running, the findings computed from the earlier bytes are no longer
  attributed to the newer file. Diagnostics for open `.rs` documents are
  tied to the bytes the analysis actually read, and a document whose read
  bytes are missing or conflicting stays quarantined instead of borrowing
  the current disk or save state (#4919).
- `ripr check --format github` no longer prints "No static exposure findings
  found" when the working tree was left unanalyzed, no scope was given, or
  a preview language was not analyzed; it leads with a warning naming the
  unanalyzed surface and how to include it. The preview-actionability and
  TypeScript card lines now say `complete repair packet` when the repair
  packet is ready, instead of always saying `no repair packet` (#5033).
