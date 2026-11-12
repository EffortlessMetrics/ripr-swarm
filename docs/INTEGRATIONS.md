# Integrations

Which tools ripr should work with, what exists today, and copy-paste setups for
the gaps. Snippets here use only commands whose behavior was run locally
(`ripr check`, `--format sarif`, exit codes); none was run on the named hosted
service. Each says so.

## Shortlist and why

Chosen from the tools a Rust team already runs on a pull request, plus the
surfaces where a developer would feel ripr in the inner loop.

| Integration | Why | Today |
| --- | --- | --- |
| GitHub Actions and code scanning | Where most Rust PRs are reviewed | `ripr init --ci github`, SARIF upload ([CI quickstart](QUICKSTART.md#ci-first-hour)) |
| VS Code, Zed, Neovim and other LSP editors | Inner loop; diagnostics on save | [Editor quickstart](QUICKSTART.md#vs-code-first-hour), [Neovim](interop/neovim-lsp.md), [others](interop/other-editors-lsp.md) |
| Coding agents (MCP) | Agents write the tests ripr points at | [MCP status server](interop/mcp.md) |
| Muse | Guided setup and one bounded test repair | [Native package guide](../plugins/muse/ripr/README.md): two skills and an explicitly approved MCP server; requires an MCP-capable RIPR build |
| pre-commit framework | Catches the gap before the push | Commit and push hooks ship in `.pre-commit-hooks.yaml`; local form below |
| lefthook, husky, plain Git hooks | Teams that do not use pre-commit | Snippets below |
| GitLab CI and other hosted CI | Not everyone is on GitHub | Snippet below; no generator |
| cargo-nextest | The standard Rust test runner | Ordering guidance below; ripr never runs tests |
| cargo-mutants | The runtime authority ripr is the cheap pre-filter for | Advisory import of existing results; external execution |
| Dev containers and Docker | Reproducible onboarding | Snippet below |
| Sibling tools and `ub-review` | The org's own PR gate | [Sibling tools](interop/sibling-tools.md) |

Not pursued now: coverage services (Codecov, Coveralls) answer a different
question; SonarQube and similar platforms have no static-exposure concept to
map to.

## pre-commit framework

The repository ships a `.pre-commit-hooks.yaml`, so a project (or a coding
agent running pre-commit) can reference ripr directly:

```yaml
repos:
  - repo: https://github.com/EffortlessMetrics/ripr
    rev: v0.11.0
    hooks:
      - id: ripr
      - id: ripr-push   # optional; needs `pre-commit install --hook-type pre-push`
```

`ripr` runs at commit time (stages `pre-commit`, `pre-merge-commit` and
`manual`), so a push-time install does not rerun it against a clean tree.
`ripr-push` runs at push time: `ripr check --committed --quiet` compares the
current branch's commits with the default base (`origin/HEAD`, then
`origin/main`, `origin/master`, `main`, `master`). That is the branch against
its base, not the exact ref range being pushed. Pushing `main` reports the
local commits not yet in `origin/main`, and "nothing was in scope" when the
base resolves to the same commit as `HEAD`. Staged and unstaged edits are left out
and the report says so. When none of those bases resolves,
ripr exits 2, the hook fails and the push stops with a message naming the
fix; for a repository whose base has another name, set
`args: [--base, origin/develop]` on the hook.

The hooks use `language: system`: `ripr` must already be on `PATH` (see
[Install channels](INSTALL_CHANNELS.md)). It deliberately does not use
`language: rust`, which would compile the whole crate in every consumer's hook
environment. The manifest is new in 0.11.0: no tag before `v0.11.0` contains
it, and `rev` must name a release tag that does. The hook runs
`ripr check --worktree --base HEAD --quiet` when a Rust file is selected, with
`pass_filenames: false` and `verbose: true` so findings show even when the hook
passes; `--quiet` keeps stderr progress lines out of the hook log. On a
`git commit` (or a plain `pre-commit run`), pre-commit stashes unstaged edits
before running hooks, so ripr checks exactly the change being committed;
`pre-commit run --all-files` does not stash, so unstaged edits are included.
New files stay out of the diff until they are staged.
Output is the default human rendering. For a much smaller terminal (about 570
bytes against about 3,400 for the same one-finding run), add arguments, which
pre-commit appends to the entry:

```yaml
      - id: ripr
        args: [--format, agent]
```

The alternative below defines the same hook locally so nothing is fetched from
this repository. `ripr check` exits 0 with or without findings and exits 2 only
when analysis could not run, so either form is advisory unless the analysis
fails.

Local form, equivalent to the shipped hook:

```yaml
repos:
  - repo: local
    hooks:
      - id: ripr
        name: ripr (static test-gap check)
        entry: ripr check --worktree --base HEAD --quiet
        language: system
        types_or: [rust]
        pass_filenames: false
        verbose: true   # show findings even when the hook passes
```

`types_or: [rust]` runs the hook when a Rust file is selected. To include
enabled Python and TypeScript/JavaScript preview adapters, replace that line
(for the shipped hook, set `types_or:` on the hook entry in your own config,
which pre-commit lets you override) with:

```yaml
        types_or: [rust, python, javascript, jsx, ts, tsx]
```

[pre-commit's file filters](https://pre-commit.com/#filtering-files-with-types)
match any listed `types_or` tag, while `types` requires every listed tag.
Use the replacement above without a separate `types: [rust]` line, which
would still exclude the other languages. These tags select when the hook
runs; they do not enable ripr's language adapters.

`--worktree` needs a 0.11 build. On 0.10 use `ripr check` after committing.
Run locally against an uncommitted edit: exit 0, "no changed line is behavior
ripr checks" for a comment-only change. File selection was checked with
pre-commit 4.7.0 on Linux using a recording entry: the default selected only
`.rs`; the expanded list selected `.rs`, `.py`, `.js`, `.jsx`, `.ts`, and
`.tsx`; both skipped Markdown. That check verifies the filter and
`pass_filenames: false`, not an installed Git hook running ripr.

The shipped manifest was exercised with pre-commit 4.7.0 on Linux using
`pre-commit try-repo <worktree> ripr` (with and without `--all-files`) and a
config naming the worktree as `repo:`, with a built `ripr` on `PATH` and a
scratch Cargo crate holding one staged edit to a branch condition plus one
unstaged edit. The hook ran `ripr check`, printed its report without progress
lines, and passed (exit 0). The default run stashed the unstaged edit and
reported only the staged change; `--all-files` reported both; a clean tree
reported "nothing was in scope" and passed. `args: [--format, agent]` and a
`types_or: [rust, python]` override were accepted and ran. No hosted service
and no released tag were involved, so `rev: v0.11.0` resolving to a tag that
contains the manifest is not yet verified.

The stage split and `ripr-push` were exercised the same way with
`pre-commit try-repo <worktree> --hook-stage pre-push --from-ref origin/main
--to-ref HEAD` (only `ripr-push` ran; it reported the committed change against
`origin/main`) and `--hook-stage pre-commit` (only `ripr` ran). In a repository
whose only branch is `trunk`, with no remote, `ripr check --committed` exited 2
with "could not resolve a default base". A test,
`crates/ripr/tests/pre_commit_hook_manifest.rs`, runs every manifest entry
through the built binary so a renamed flag fails the build's tests instead of
every consumer's hook.

## lefthook, husky and plain Git hooks

The same two commands work in any hook manager; `ripr` must be on `PATH`.

[lefthook](https://lefthook.dev), `lefthook.yml`:

```yaml
pre-commit:
  commands:
    ripr:
      glob: "*.rs"
      run: ripr check --worktree --base HEAD --quiet
pre-push:
  commands:
    ripr:
      glob: "*.rs"
      run: ripr check --committed --quiet
```

[husky](https://typicode.github.io/husky/) 9, `.husky/pre-commit` and
`.husky/pre-push`, one line each:

```sh
ripr check --worktree --base HEAD --quiet
```

```sh
ripr check --committed --quiet
```

Without a hook manager, `.git/hooks/pre-commit` (made executable):

```sh
#!/bin/sh
exec ripr check --worktree --base HEAD --quiet
```

All three are advisory for the same reason as the pre-commit hook: findings
exit 0, and only a run that could not analyze exits 2. They differ in what a
commit-time run sees. pre-commit stashes every unstaged edit, so its report
covers exactly what is being committed. lefthook hides only the unstaged part
of a partly staged file, so edits to other tracked files that are not staged
at all still appear. husky and a plain hook hide nothing, and with no file
filter they also run, and print a report, on commits that touch no Rust file.

Exercised on Linux with lefthook 1.13.6 and husky 9 from npm, in a scratch
Cargo crate with a bare local `origin`. A real `git commit` and `git push` ran
each hook. lefthook's commit run hid the unstaged part of a partly staged file, and its push
run compared against `origin/main`. With a second, wholly unstaged file,
lefthook's commit report included that file too. husky's commit run included
an unstaged edit, as did the plain hook. All exited 0.

## GitLab CI and other hosted CI

Install the prebuilt archive, verify its checksum, run, keep the report.
Replace `0.11.0` with the release you pin. The archive name is the one the
release workflow publishes; 0.11.0 assets must exist first (see
[Install channels](INSTALL_CHANNELS.md)).

```yaml
ripr:
  image: rust:1
  variables:
    GIT_DEPTH: "0"          # ripr needs the base commit; a shallow clone fails with exit 2
    RIPR_VERSION: "0.11.0"
  script:
    - base="https://github.com/EffortlessMetrics/ripr/releases/download/v${RIPR_VERSION}"
    - file="ripr-server-v${RIPR_VERSION}-x86_64-unknown-linux-gnu.tar.gz"
    - curl -fsSLO "$base/$file" && curl -fsSLO "$base/$file.sha256"
    - echo "$(cat $file.sha256)  $file" | sha256sum -c -
    - tar xzf "$file"          # members are ./ripr, so extract everything
    - ./ripr check --base "origin/$CI_MERGE_REQUEST_TARGET_BRANCH_NAME" --format sarif > ripr.sarif
    - ./ripr check --base "origin/$CI_MERGE_REQUEST_TARGET_BRANCH_NAME"
  artifacts:
    paths: [ripr.sarif]
  rules:
    - if: $CI_PIPELINE_SOURCE == "merge_request_event"
  allow_failure: true
```

The example covers merge requests from branches in the same project; a fork's pipeline runs in the fork, so fetch the target project's branch (`git fetch <target-url> <branch>`) and pass that ref to `--base`. The same four steps (fetch full history, install, verify, `ripr check --base`)
apply to Buildkite, CircleCI and Jenkins. Not run on a GitLab runner. The
shallow-clone note comes from ripr's own exit-2 message, which names the fix.

## cargo-nextest

ripr reads source; it does not execute tests, so it does not depend on nextest.
Order the two so the cheap one speaks first:

```bash
ripr check --base origin/main     # seconds, static
cargo nextest run                 # then the real run
```

ripr's "related tests" are found by name and call graph, not by nextest
filter sets. A finding that recommends a test is a prompt to write one; it is
not a claim that the nextest run would pass or fail.

## cargo-mutants

Switching from cargo-mutants? See [Coming from cargo-mutants](FROM_CARGO_MUTANTS.md).

ripr answers "does a test appear to contain a discriminator for this change"
without running anything. cargo-mutants answers it by running mutants. Use ripr
to see where a change has no discriminator, add the test, then let cargo-mutants
confirm on the files you changed:

```bash
ripr check --base origin/main
git diff origin/main > pr.diff
cargo mutants --in-diff pr.diff
```

After the external run, `ripr calibrate cargo-mutants` can import supplied
`outcomes.json` or a `mutants.out` directory and join the results to a
`repo-exposure-json` snapshot as advisory calibration. Run
`ripr calibrate cargo-mutants --help` for cargo-mutants input-production steps
and import options. The importer does not launch cargo-mutants, change static
classifications, or configure CI policy. `--in-diff` is cargo-mutants' own flag.

Keep the supplied artifacts tied to the intended repository and source state.
The importer matches by seam identity, span, or file/line; it does not verify
that both inputs describe the same repository, commit, or working tree.
Matching paths or heads alone cannot establish that provenance.

A `mutants.json`-only import contains unexecuted candidates, not observed
runtime results. Those rows remain `unknown` / `no_runtime_data`; use
`outcomes.json`, or a directory containing it, for observed outcomes.

## Dev containers

```json
{
  "image": "mcr.microsoft.com/devcontainers/rust:1",
  "postCreateCommand": "cargo install cargo-binstall && cargo binstall -y ripr"
}
```

From 0.11.0 on, binstall downloads the prebuilt archive; before that it falls
back to compiling. Not run in a dev container.
