<!-- section: Fixed -->
- `ripr cache clear` refuses and removes nothing when a cache layer could
  not be fully inspected, and names the layer and its inspection state
  (#2919).
- A dot inside a double-quoted string literal in an assertion
  (`assert_eq!(msg, "error.timeout")`) is no longer read as a field access,
  so such an assertion is not credited as a field observer for a
  struct-construction change (#2937, #2904).
- Rust workspaces: when the index spans the whole workspace, a test in
  another workspace crate that calls the changed function directly is now
  listed as a related test, provided no other indexed function or method
  shares that name. Before, every test outside the owner's package was
  dropped before its calls were considered, so such an integration test
  was never credited. A same-named test in another crate that does not
  call the function is still excluded (#3014, #3047, #2971, #3033).
- The portable input identity in `repo-exposure-json` artifacts now reads
  Cargo manifest and lockfile CRLF line endings as LF, so the same commit
  checked out with CRLF and with LF line endings gets one identity and
  `ripr agent verify` no longer refuses such a before/after pair as
  incomparable. Standalone CR is kept (#3119).
- `ripr --version` (or `-V`) given before any command now prints only the
  version, even when `--json`, `--help` or `--verbose` is also passed, and
  no longer prints the `ripr: verbose mode enabled` line. A command's own
  flag such as `ripr lsp --version` still reaches that command (#3159).
- A gap record whose verification or receipt command is blank or
  whitespace-only, or whose command list mixes a real command with a blank
  one, is no longer treated as a complete repair route: `ripr reports
  gap-ledger` shows `Verify: unavailable_incomplete_command_list` and counts
  the gap as not eligible for PR comments, gate candidates or agent packets,
  and the language server and review comments stop treating it as a complete
  repair packet (#3250, #3249).
- `ripr pr-evidence` now exits with an error after it writes a producer
  error packet (`status: "error"`), saying the PR evidence producer failed
  and `review-comments` must not run, instead of exiting successfully, and a
  later check of that packet refuses it too. The library exports the same
  rule as `ripr::reject_pr_evidence_error_packet` (#3264).
- Rust: a change to a helper function inside an inline `#[cfg(test)]`
  module is no longer reported as a production finding. The helper is
  read as test evidence, so editing test plumbing no longer produces a
  finding (for example `weakly_exposed`) that asks for a test of the
  helper itself (#3273).
- Rust related tests no longer list a test as `same_test_file` only because
  its path contains the changed file's stem: `src/config.rs` no longer pairs
  that way with `tests/reconfigure.rs`, which now relates as low-confidence
  `weak_token_substring`. The file's own stem and its `_test` and `_tests`
  companions still count as the same test file (#3478, #3474).
- On Unix, a file name that is not valid UTF-8 no longer earns a
  file-name match with a different file whose name renders the same after
  lossy decoding (for example `pricing_<0xFF>.rs` and `pricing_<0xFE>.rs`);
  such a name now gives no file-name affinity when ripr relates tests to
  changed code (#3600).
- `ripr check --worktree` now asks Git for a plain zero-context source
  patch like the other diff routes: external diff helpers, `textconv`
  filters, `color.diff` and context settings from the repository's Git
  config no longer hide changes, invent source coordinates or widen a
  one-line edit to the whole file. `--no-textconv` and `--no-color` now
  apply to every route (#3857, #3850).
- `ripr cache clear` now removes only the recognized ripr cache layers
  inside the cache directory and never deletes the directory itself or
  unrelated files beside the layers. It refuses with no files removed when
  the directory holds files but no recognized layer, or a layer or path
  component is a symlink, is not a directory, or cannot be inspected
  (#3865, #3843).
- `ripr check --base <ref>` with a ref the repository does not have (for
  example `origin/main` with no `origin` remote) now fails naming the ref,
  saying the analysis did not run and giving the next actions, instead of
  printing Git's `ambiguous argument` advice to separate paths from
  revisions (#3876, #3875).
- `ripr check --root PATH` now refuses an explicit root that does not exist
  or is not a directory with the same root error `rerun` and `agent` give,
  instead of printing the failed `git diff` invocation with its full argument
  list (#3882).
