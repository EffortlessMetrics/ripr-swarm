<!-- section: Fixed -->
- `ripr pr-evidence` now records the PR diff with the same pinned Git
  options the analysis uses (no external diff, textconv or color), so a
  repository textconv driver or color setting can no longer hide or alter
  source edits in the packet's diff. The diff keeps three context lines
  and still refuses non-UTF-8 Git output (#3994, #3930).
- `ripr doctor` now reports a failing `git_repository` check when the root
  is not inside a Git work tree, or when that cannot be determined, instead
  of passing (#4007, #3988).
- Generated GitHub workflow (`ripr init --ci github`): the first-pr
  start-here step now passes the pull request's base branch (the default
  branch on a manual run) instead of assuming `origin/main`, so a repository
  whose pull requests target another branch no longer gets a blocked
  start-here. PR guidance annotations name the repair start for a card past
  the repair-packet check instead of a brief command that pointed into the
  runner's own checkout path (#4017).
- `ripr first-pr` on a Rust root no longer reports "no actionable gap"
  from review cards it could not use. Missing cards stop with
  `missing_artifact`, and unreadable, incomplete, or other-root, base or
  head cards stop with a blocked state; both name the `ripr review-comments`
  command that regenerates them (#4019).
- Python preview analysis now collects test functions and methods that use
  the default pytest and unittest `test` name prefix without an underscore
  (for example `testParse`), for both `def` and `async def`, so their
  assertions can be credited. Before, only `test_*` names were read as
  tests. A custom `python_functions` pattern does not add other names to
  the collected tests, and a changed `testMethodPrefix` is not read
  (#4066).
- Repair packet eligibility compares a seam's required discriminator with
  its missing-discriminator facts case-sensitively, so a fact naming an
  identifier or literal that differs only in letter case (`limit` for
  `LIMIT`) no longer makes the seam eligible for a repair packet (#4069).
- VS Code: a release-candidate build of the extension can now install its
  managed server before the stable release exists. When the stable
  manifest request returns a direct, non-redirected 404, the extension uses
  the requesting release's own RC manifest only if its bytes match the
  embedded manifest digest and its generation and target match; a redirect,
  timeout, auth or server error, or digest or identity mismatch still fails
  closed (#4083, #3798).
- The `ripr agent repair --phase after` transaction receipt now sets
  `test_changed` to the selected test file path when the edit cage found the
  edit compliant and recorded that exact file changing, instead of always
  `null`. The targeted-test outcome no longer says `<stage> evidence moved
  from X to X`: a stage whose state did not move reads as stayed and names
  whether its confidence or only its summary changed (#4085).
- `ripr check` now pins `--src-prefix=a/ --dst-prefix=b/` on the `git diff`
  it runs, so a `diff.noprefix` setting in the repository or user Git
  configuration no longer changes the file paths ripr parses (for example a
  file under a top-level `b/` directory losing that segment). The diff
  capture in the `ripr init --ci github` workflow pins the same prefixes
  (#4087, #4086).
- TypeScript checks: a test that calls the changed function inside a spread
  (`[...steps(4)]`) now relates to it as a call, instead of the finding
  reading `no_static_path`. Member calls such as `obj.steps(4)` and
  `obj?.steps(4)` still do not count (#4114, #4104).
- VS Code: stopping the language client during a restart or Workspace
  Trust transition now disposes only that client's notification handlers,
  so a replacement client keeps its own. A startup failure whose cleanup
  stop also fails now reports both errors and keeps the client available
  for an explicit stop, instead of discarding the cleanup failure (#4188).
- The first-useful-action Markdown's one-screen `Changed behavior` line now
  shows the changed expression the selected evidence names, or says it is
  not named, with the explanation on its own `Why:` line, and the JSON
  carries the expression as `selected.changed_behavior` (#4240, #4010).
- `ripr first-pr` no longer promotes a summary-only review card for code
  outside the PR's changed lines to its top actionable gap; it says how
  many such cards carry a repair start and points at `ripr pilot` for
  repository-wide repairs. For a blocked gap ledger in a Rust root, the
  retry now regenerates `repo-exposure` before the ledger, with every path
  anchored at `--root`, instead of repeating the command that produced
  the blocked ledger (#4253).
- Rust: a boundary written with the literal on the left (`100 < amount`)
  or held in a local (`let limit = 100; amount > limit`) is now read at its
  boundary value in both `ripr check` and repository seam grip, so a test
  that passes the boundary value is credited and one that passes only a
  value one step off is not. A boundary local ripr cannot evaluate is
  reported as a static limitation instead of naming a repair that could not
  change the reading (#4259, #4228).
- TypeScript and JavaScript preview: an owner-module mock is now recognized
  when the runner is a renamed import (`import { vi as v } from "vitest"`),
  when the specifier is a typed `import("../src/pricing")`, a root-relative
  path (`/src/pricing`) or a directory whose `index` module is the owner, and
  when the specifier cannot be resolved. An assertion in such a test is no
  longer credited as observing the changed owner; the finding keeps the
  `mocked_module` static limit and stays advisory (#4305, #4294).
