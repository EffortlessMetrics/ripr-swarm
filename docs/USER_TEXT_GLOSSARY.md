# User text glossary

Default `ripr` output is being moved to plain language, surface by surface.
Internal terms stay available where they carry meaning for integrators:
`--verbose`, `--format human-full`, JSON, SARIF, schemas and specs. This page is
the single list of terms the default human output should not use, the wording
to use instead, and where the internal term may still appear. `cargo xtask
check-user-text` does not make the output jargon-free today; it stops the
banned-term counts from growing while the surface issues remove them.

Rule: refusals and user-facing messages use plain wording unless a term is
already very clear to a first-time user. Wording stays within the conservative
static vocabulary: `ripr` reads code and reports what tests appear to check;
it does not run tests or mutants and claims no runtime outcome.

For the public-to-internal bridge used in marketing and README copy, see
[Terminology](TERMINOLOGY.md). This page covers the in-product text.

## Banned in default human output

| Internal term | Plain wording | Internal term may still appear in |
| --- | --- | --- |
| probe | changed behavior checked; change point | JSON, `--format human-full`, finding IDs such as `probe:src_lib.rs:...` (copy-paste identifiers are exempt) |
| seam | gap; spot | JSON, `--format human-full`, `--seam-id` flag spelling (exempt) |
| discriminator, missing discriminator | missing check; a test that asserts the result for this input | JSON, `--format human-full`, spec documents |
| edit cage | files you may change / do not change | JSON, `--verbose`, specs |
| admitted, admission | found; accepted | `--verbose`, JSON |
| delegatable | ready to hand to an AI agent | JSON, specs |
| typed refusal, typed error, fails closed | ripr declined and says why; stops with an error that names the cause | specs, `docs/EXIT_CODES.md` |
| `RIPR-SPEC-` ids | a docs link, or nothing | specs, `--verbose`, docs |
| snake_case state tokens (`weakly_exposed`, `reachable_unrevealed`, `no_static_path`, `infection_unknown`, `propagation_unknown`, `static_unknown`, `weakly_gripped`, `ungripped`, `partial_with_limitations`, `language_adapter_unavailable`, `unsupported_or_limited`, `choose_item`, `select_item`, `allowed_edit_surface`, ...) | the plain word: exposed, weak, reached but not noticed, no test path, unclear | JSON, `--format human-full`, schemas |

The exact banned list lives in `xtask/src/policy/user_text.rs`. Adding a term
means adding a row here and a needle there in the same PR.

Terms that are fine as they are: exposed, weak, no path, static, advisory,
changed behavior, related test, Next step, base/head, doctor, pilot.

## What the check scans

`cargo xtask check-user-text` reads real output, not source strings:

- every committed default-format golden, `fixtures/**/expected/human.txt`;
- live output of the built `ripr` binary in a scratch Git repository:
  `check` (also outside Git), `pilot`, `doctor`, `status`, `continue`,
  `repair` refusals (missing `.gitignore` entry, uncommitted source change),
  `agent card` (unknown ID and a real gap), `agent packet`, `agent repair`,
  and the help screens (`--help`, `help --all`, and `help <command>`).

Explicitly not scanned (the allowlist printed in every report):

- `fixtures/**/expected/human-full.txt` and any `--format human-full` output;
- any `--verbose` / `-v` output;
- JSON, SARIF and other machine formats;
- `docs/specs/**` and other internal reference documents.

Each live surface asserts a distinctive line it must contain (a help screen's
usage header, a refusal's opening sentence), so a wrong binary or a failed
scratch setup is an instrument failure, never a pass. The goldens carry no such
marker; they are only required to exist (zero goldens fails the check).

The live half uses the default-feature `ripr` binary (`cargo build -p ripr`);
text that only appears under non-default features is not scanned. It builds the
binary and runs about 25 commands, so locally a change that touches nothing under
`crates/ripr/`, the root manifests and toolchain files, `.cargo/`, or this check
and its baseline scans the goldens only (`precommit`). Hosted CI (`ci-policy`,
`check-pr`), `--full`, `--ripr` and `--ratchet` always run both halves.

## Baseline ceilings

`main` still contains some banned terms in default output. They are recorded by
`policy/user-text-baseline.toml`, one ceiling per surface and term: the number
of output lines containing the term when last lowered.

- Golden rows (`golden-human`) gate each PR on its own delta (#11374), per
  file. The check scans the `expected/human.txt` goldens at the merge-base with
  `origin/main` through Git objects (no base build) and fails when any
  `(file, term)` has more banned lines than at the merge-base. A file that is
  new at the head counts all its banned lines as an increase, and removals in
  one file never offset a rise in another. A rename or move that keeps old
  jargon therefore reads as a new file and fails: a deliberate false red, the
  safe direction; reword the lines in the same change. A PR never edits the
  baseline to pass, and a parallel merge cannot turn another PR red.
- What the golden gate prevents and allows, precisely. Prevented: a new banned
  line in a golden file that is new at the head, and any per-file, per-term
  increase in an existing golden. Allowed: pure removals, edits that keep a
  file's count equal or lower, and renames or moves of goldens that carry no
  banned lines. It is therefore not a plain net no-growth rule: removals never
  offset increases elsewhere, and a rename of a golden that still carries banned
  lines is rejected as a new path.
- Inputs are recorded. The report names the merge-base commit, where
  `origin/main` pointed, and HEAD, and says when the head side is the working
  tree (uncommitted golden changes); a stale `origin/main` is visible there.
- Unreadable base: locally the check falls back to absolute ceilings and labels
  the run "base unavailable: absolute-ceiling check only, no PR-relative
  no-growth established" (a loose ceiling can pass growth). Under CI (`CI=true`
  or `GITHUB_ACTIONS` set) it fails closed instead.
- A golden total above its absolute ceiling is reported as a WARNING (main
  health) and does not fail a PR that did not add lines.
- To restore the delta gate after an unreadable base, run
  `git fetch --deepen=200 origin main` (or `git fetch origin main`).
- Live-surface rows are small and keep absolute ceilings: a live count above
  its ceiling fails, and a PR that raises a live-surface ceiling needs an
  explicit, reviewed reason. A term with no row has a ceiling of 0.
- Known gaps, tracked as follow-ups: the gate logic itself is reviewed by hand
  (a PR that edits it is not qualified by the gate it changes), and until a
  main-side backstop exists, golden ceilings are warnings on `main`, not
  failures.
- Ceilings only go down. After fixing wording, run
  `cargo xtask check-user-text --ratchet` to lower them; it never raises or
  adds a row, and it does not rewrite while any count is above its ceiling. A
  ceiling that is higher than the current count does not fail; the report prints
  these rows as a NOTE. Raising a ceiling needs a stated reason in the PR and is
  a review blocker by default.
- Surface issues remove their terms: #10691 (check), #10692 (repair and agent
  refusals), #10693 (agent card), #10694 (pilot), #10695 (editor).

## Changing human output: the golden bless path

Wording changes reach committed goldens (`expected/human.txt`,
`expected/human-full.txt`, `check.json`). The flow:

1. Change the renderer wording in `crates/ripr/src/output` or `crates/ripr/src/cli`.
2. `cargo xtask goldens check` lists every fixture whose output drifted.
3. For each drifted fixture, bless it with a reason:
   `cargo xtask goldens bless <name> --reason "#NNNN: plain wording for <term>"`.
   A family of fixtures with a shared prefix can be done in one shell loop:

   ```bash
   for name in $(ls fixtures | grep '^ts_'); do
     cargo xtask goldens bless "$name" --reason "#NNNN: plain wording for <term>"
   done
   ```

   Each bless appends to that fixture's `expected/CHANGELOG.md`. Review the
   diff of `human.txt`: only the intended wording should move.
4. `cargo xtask check-user-text --ratchet` lowers the ceilings the change
   earned, then `cargo xtask precommit` must pass.

There is no single repository-wide bless command on purpose: each fixture is
blessed with a reason so that a wording sweep stays reviewable.
