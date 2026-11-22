<!-- section: Fixed -->
- `ripr agent card` now gives the same `repair_card_id` and digests for the
  same commit checked out at different paths, instead of folding the
  absolute checkout root into the card identity. Its currentness now reads
  `accepted_dirty_draft` rather than `current` when the seam's file, a
  related test file or the recommended test file has uncommitted changes,
  ignored edits included, and no card is produced if that status check
  fails (#5035, #5005, #5008).
- LSP: a `refreshDeadlineMs` below 100 ms is now rejected with a warning on
  both the initialization and pulled-configuration paths instead of
  cancelling every refresh before it starts. A refresh stopped by its
  deadline now logs one `window/logMessage` warning naming the deadline and
  sets `analysisStatus.failure.kind` to `deadline_exceeded`, so editors
  without work-done progress no longer see the drop silently (#5093).
- `ripr first-pr` start-here and the PR summary now show the verify and
  receipt commands of a gap-ledger selection as `(cd -P -- <root> && ...)`,
  so they run against the selected root from any directory, and withhold a
  PowerShell form with recovery advice instead of claiming the raw line
  runs unchanged there. JSON adds `selected.command_context`; the raw command
  fields are unchanged (#5108).
- The PR evidence summary's Local Reproduction Commands section no longer
  presents an earlier repair-start or reproduction-check command as the
  verification step when its text matches the verify command; only the
  final verification entry gets that context. JSON output is unchanged
  (#5120, #5118).
- `ripr pr-summary` and `ripr first-pr` Markdown now print the rooted Bash
  form `(cd -P -- '<root>' && <command>)` for a verify or receipt command
  with a literal `=` argument (such as `--test-threads=1`). Such a command
  previously got no rooted Bash form because it could not be translated to
  PowerShell (#5121, #5119).
- `ripr first-pr --check` now refuses a start-here packet whose carried
  command context names a directory other than the current root (a
  relocated packet, a missing directory or invalid context) and prints a
  root-bound refresh command, instead of showing commands for the old
  location. A symlink to the same physical root is still accepted
  (#5123, #5117).
- `ripr first-pr` refresh commands now name the selected root by its
  resolved physical path, including a root reached through a symlinked
  parent, and `ripr first-pr --check` accepts a packet whose existing root
  directory name contains a newline when the packet withholds its shell
  command forms, instead of rejecting it (#5136, #5135).
- A changed source file that is a symlink, or that sits below a symlinked
  or non-directory path under the selected root, now gets the
  `changed_file_absent_from_worktree` limitation and its probes are
  withheld, matching what source discovery reads, instead of being treated
  as present. The selected root itself may still be a symlink (#5142).
- `ripr first-pr` now quotes the root, base, head and branch in its
  preflight recovery commands and stale-packet and missing-base messages,
  and its fetch hint ends git option parsing before the branch. Before, a
  root with spaces split into several arguments and a crafted ref name could
  run another command when the hint was pasted (#5188).
- `ripr pr-evidence --root` naming a directory outside the repository it
  runs from is now refused before any Git read or write. Before, it diffed
  the current repository, analyzed the other root's source, and wrote a
  packet reporting no changed files into the wrong tree with exit 0. A
  missing or file `--root` gets a named refusal instead of Git's `not a git
  repository` (#5225).
- The PowerShell form of a printed command now doubles a typographic single
  quote (U+2018 to U+201B) inside a quoted path or ref, so a root such as
  `Steven’s repo` pastes as one argument instead of ending the quoted
  string early, where a crafted name could run a second command. The
  `ripr check` drill-in and the `ripr explain` next step now also print a
  `(PowerShell)` line when that form differs from the bash one (#5232).
- `ripr pilot` now honors `[languages] enabled` in `ripr.toml`: when Rust
  is not enabled it skips the Rust seam scan instead of analyzing it anyway,
  says the result is not clean, and its JSON carries a
  `rust_excluded_from_scope` entry. `ripr agent status` stops on that pilot
  result with the config-edit guidance instead of sending the caller back
  to `ripr pilot` (#5205, #5248).
- `ripr check --format human-full` and `ripr explain` now wrap a multi-line
  changed fragment one source line at a time, so a short line such as
  `build: BuildMetadata::EMPTY,` is no longer split mid-token once the
  whole fragment passes the column budget (#5293).
- Python: assertions in a test carrying a recognized pytest `skipif` or
  `xfail` mark, or a unittest `skipIf`, `skipUnless` or `expectedFailure`
  control in a placement the runner honors, no longer credit an oracle or
  boundary input; the test stays listed as related with
  `test_activation_unestablished`. When no other active test relates, the
  finding reports the activation reason and emits no repair packet
  (#5414).
- MCP: on a session that began with `initialize`, a `ping` request whose
  `params._meta` carries handshake fields is now answered with an empty
  result instead of a method-not-found error; a discovery-mode session still
  answers `ping` as method-not-found (#6236).
