# Troubleshooting

Use this page when `ripr` reports nothing, stops, or reports something you did
not expect, and the screen alone does not tell you why.

Every line in a `text` block below is text `ripr` prints: a whole output line,
or the stable part of a longer one that also carries your paths and names. A
test (`crates/ripr/tests/troubleshooting_guide.rs`) reproduces each situation
with the built binary and fails when quoted text stops appearing, so this
page cannot drift from the product.

## Read these three things first

Most surprises are explained by three parts of the `ripr check` screen.

1. **`base:` and `head:`** near the top say what was compared. A wrong base
   or an unexpected `head: HEAD` (committed history only) explains most empty
   results.
2. **`Analysis outcome:`** says whether the analysis was complete and why the
   result is what it is. Its `Counts:` line lists changed files, changed
   lines, candidate lines (changed lines ripr considers) and findings.
3. **The exit code.** `0` means the command completed, with or without
   findings. `2` means it could not complete. `3` means ripr declined or a
   gate blocked. See [Exit codes](EXIT_CODES.md).

If the setup itself is in doubt, run `ripr doctor`. Lines marked `!` are
failures and each one carries a `Fix:` line.

## ripr reports nothing

An empty result is never proof that the change is well tested. Find the line
that matches your screen.

### The diff is empty

```text
Nothing to check: the diff against the comparison base is empty (no changed files), so ripr examined no behavior.
Safe next action: you are on `main`, which is the comparison base; edit a tracked source file or switch to a feature branch
```

`ripr` compares your work against a base branch. On the base branch itself
with a clean tree there is nothing to compare.

- Edit a tracked file, or switch to your feature branch.
- To look at work that is already committed on the base branch, pass the
  commit before it: `ripr check --base <commit before the change>`.

### A new file is not picked up

```text
run `git add -N <paths>` then `ripr check --worktree`, or pass `--diff PATH`
```

Untracked files are not part of any Git diff, so `ripr` cannot see them. The
`untracked:` line names the files. Run `git add -N <path>` (intent to add) or
`git add <path>`, then run `ripr check` again.

### Only comments or tests changed

```text
Analysis outcome: no changed line is behavior ripr checks (analysis complete)
No findings: ripr checked no changed behavior.
```

The diff has changed lines, and none of them is production behavior that
`ripr` checks. Comment edits, whitespace and changes that touch only test code
produce no finding. This is the expected result for a test-only commit.

If you expected a finding, the base probably already contains your production
change. This happens when you commit the code, then the tests, and compare
against the commit in between. Choose a base before the production change:
`ripr check --base <commit before the change>`.

### Only documentation or configuration changed

```text
Nothing to check: no changed source file in a language ripr analyzes was available, so ripr examined no behavior.
```

Files were changed, and none is source code in a language `ripr` reads. No
action is needed.

### The changed language is in preview and not enabled

```text
The Python adapter is preview and not enabled, so these files were not analyzed
To enable, add to ripr.toml:
enabled = ["rust", "python"]
```

Python and TypeScript support is a preview that a repository opts into. The
note prints the exact `ripr.toml` block for your repository. Add it and run
`ripr check` again. Results from a preview language are advisory. See
[language configuration](CONFIGURATION.md#languages).

### Rust is switched off in `ripr.toml`

```text
Not analyzed: 1 changed file(s) in a language ripr is not set up to analyze.
Fix: The effective [languages].enabled set is ["python"], which excludes rust; add "rust" to [languages].enabled in ripr.toml and re-run the analysis.
```

A `[languages]` list replaces the defaults, so a list that names only a
preview language turns Rust off. Add `"rust"` to the list.

### The changed language is not supported

```text
ripr analyzes Rust, plus TypeScript/JavaScript and Python as previews; review these changes with their own tests.
```

The same line names the files that were left out. `ripr` has nothing to say
about them, so the analysis is reported as incomplete.

### Findings exist but are withheld

```text
Withheld by owner opt-out (owner_mutants_skip): 1 finding(s); the changed owner carries `#[mutants::skip]`, so ripr does not list it as a gap. Remove the attribute to see these findings.
```

`ripr` honors `#[mutants::skip]` on a changed function. The summary line also
counts findings hidden by a suppression file as `suppressed by your settings`.
Remove the attribute or the suppression entry to see them again. See
[Coming from cargo-mutants](FROM_CARGO_MUTANTS.md).

### Every finding reads `exposed`

```text
Static exposure: exposed
```

This is the good result: a related test appears to notice the change. `ripr`
still lists the finding so you can see which test it matched. It is static
evidence, not a test run.

## ripr stops with an error

### The base does not resolve

```text
does not resolve to a commit (the analysis did not run)
```

Exit code `2`. The message names the ref. Check its spelling with
`git branch --list`, fetch it, or pass `--base <ref>` for a ref the
repository has.

In CI the usual cause is a shallow clone. A `--depth` clone also tracks only
the branch it cloned, so ripr asks you to widen `origin` before fetching:

```text
This is a shallow clone that tracks a single branch: widen it with `git remote set-branches origin "*"`, then fetch the missing history with `git fetch --unshallow origin`
```

In GitHub Actions, set `fetch-depth: 0` on actions/checkout instead.

### The directory is not a Git repository

```text
This root is not inside a Git work tree (the analysis did not run).
```

Exit code `2`. `ripr check` needs Git history to find the change. Run it
inside the repository or pass `--root <path>`. Without Git, `ripr pilot
--root .` inspects the workspace as it is, and `ripr check --diff <file>`
reads a patch you supply.

### The root does not exist

```text
is not a directory; pass the directory that contains the workspace
```

Exit code `2` from `ripr check`. `ripr doctor` reports the same problem as a
failed check, with exit code `3`:

```text
! root directory does not exist at
Fix: create the directory or pass an existing path with `--root`
```

### The root is a subdirectory of the crate

```text
Run ripr from a Cargo workspace root (or restore the missing Cargo.toml), then re-run the analysis.
```

`--root` must point at the directory that holds `Cargo.toml`, not at `src`.
When you run `ripr` without `--root` from inside a crate, it finds the
nearest `Cargo.toml` on its own.

### `ripr.toml` is rejected

```text
invalid ripr.toml
Fix the error above in ripr.toml and rerun
```

Exit code `2`. The message quotes the line and names the allowed keys.
Configuration is optional, but an invalid file is never ignored. Run
`ripr config validate` after editing. See [Configuration](CONFIGURATION.md).

### A supplied diff names files that are not in the tree

```text
a changed file is absent from the working tree during language analysis
```

`ripr check --diff <file>` reads the patch against the files under the root.
If the patch was made in another directory or with a path prefix, its paths
do not match. Create the patch from the repository root, or pass the matching
`--root`.

## A finding looks wrong

`ripr` reads code; it does not run tests. Its findings are static evidence
and can be wrong in both directions. See [Static limits](STATIC_LIMITS.md) for
what the analysis cannot see.

### "no path" although a test covers the code

```text
Why no path: no related test was found that reaches this change
this is not a coverage assessment
```

`ripr` found no test that it can connect to the changed function by reading
the code. Tests that reach the code through macros, helper-call chains,
integration tests the static model does not trace, or another language are
the usual causes. If such a test
exists, the finding is a limit of the analysis, not a gap in your tests. If
none exists, add a test next to the code.

### "weak" although the test has assertions

```text
Why weak: a related test reaches this change, but no assertion tells the old and new behavior apart
```

The related test runs the changed code, and its assertions would pass with
both the old and the new behavior. The `Missing check:` line names the input
or value that would tell them apart. `ripr explain <file:line>` shows the
evidence for one finding and `ripr context --at <file:line>` shows the code
and test around it.

### `explain` says no finding matched

`ripr explain` and `ripr context` select a finding by its id or by the
`file:line` that `ripr check` printed. When a location matches nothing, the
command stops with exit code `2` and the message ends with the cause:

```text
No finding is at line
is not the path of a file from the analyzed root
exists, but this run has no finding anywhere in it
```

- The first names the lines in that file that do have a finding. Retry with
  one of them.
- The second means the text is not a file path from the analyzed root. Give
  the path from that root (`src/lib.rs:12`, not `lib.rs:12`), check the
  spelling, or pass `--root`.
- The third means the file exists and this run reported nothing in it.
  Findings attach only to changed lines whose behavior `ripr` checks, so an
  unchanged file has none. When the analysis was incomplete, the message says
  so, because the file may not have been analyzed at all.

## The editor shows nothing

Run `ripr: Show Status` in VS Code, save the file and use the refresh action.
The editor reads saved files, so an unsaved edit has no diagnostics yet, and
the causes on this page apply there too. See
[Editor extension](EDITOR_EXTENSION.md) and
[Server provisioning](SERVER_PROVISIONING.md).

## Still stuck

Open a [first run report](https://github.com/EffortlessMetrics/ripr-swarm/issues/new?template=first_run_report.yml)
or a [wrong finding report](https://github.com/EffortlessMetrics/ripr-swarm/issues/new?template=wrong_verdict.yml)
and include the output of `ripr --version` and `ripr doctor`.
