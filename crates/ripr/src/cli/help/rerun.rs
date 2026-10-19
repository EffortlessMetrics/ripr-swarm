pub(super) const RERUN_HELP: &str = r#"Re-evaluate static evidence affected by one edited Rust test.

Usage: ripr rerun --changed-test PATH[::TEST_NODE] [--before PATH] [--root PATH] [--json] [--out PATH]
       ripr rerun --gap CANONICAL_GAP_ID --gap-ledger PATH [--before PATH] [--root PATH] [--json] [--out PATH]

Options:
  --changed-test PATH[::TEST_NODE]
                       Edited Rust integration or unit-test file, optionally narrowed to one parsed test node. Required unless --gap is used.
  --gap ID             Canonical gap identity from the explicit gap ledger.
  --gap-ledger PATH    Gap decision ledger required by --gap.
  --before PATH        Explicit prior targeted-rerun, repo-exposure, or compatible static snapshot.
  --check-parity       Compare selected current seams with a full static inventory; explicit and potentially expensive.
  --root PATH          Workspace root. Defaults to the current directory, then
                       walks up to the enclosing workspace or package, or the git
                       top level; `--root .` pins this directory.
  --json               Emit the structured current-state report.
  --out PATH           Write the report to a file instead of stdout.

Build the explicit ledger before using --gap (examples from the repository root):
  Rust: run `ripr check --format repo-exposure-json` and save stdout as repo-exposure.json.
        ripr reports gap-ledger --repo-exposure repo-exposure.json --out gap-ledger.json
  TypeScript: repeat the check that produced the gap with --json and save stdout as check.json.
        ripr reports gap-ledger --check-output check.json --out gap-ledger.json
  Then: ripr rerun --gap CANONICAL_GAP_ID --gap-ledger gap-ledger.json
Use the same --root for capture, ledger generation, and rerun when working elsewhere.
Keep the originating check's --base/--diff/--worktree scope for check.json;
the ledger must contain the selected canonical gap ID. Rust diff findings and
repo-inventory gaps can use different IDs: ledger generation does not translate
an absent ID. A missing selection returns canonical_gap_unresolved, not a rerun
of unrelated seams. See `ripr help reports` for the ledger source options.

The changed-test selector recomputes only seams owned by uniquely resolved
functions directly called from the selected test file or test node. An unknown
or ambiguous node is a named limitation. The gap selector groups
every supplied ledger record with that canonical identity, scopes recomputation
to each unique anchored file/owner, and returns only current seams with that
same domain ID. Missing, stale, anchorless, or root-mismatched ledger selections
become named limitations; a stale record does not hide other current scopes.
RIPR never scans unrelated seams as a fallback. A --gap-ledger that cannot be
read or parsed exits 2 without a report, like an unreadable --changed-test.

Both selectors reuse valid per-file facts. Without --before the receipt reports
current_state_only and never infers movement. With an explicit compatible
before artifact it reports closed, improved, unchanged, or regressed static
movement for every selected current seam; a missing or incompatible before
artifact is a named limited result. RIPR never discovers an ambient report as
before state.
"#;
