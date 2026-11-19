# Security Policy

## Supported Versions

`ripr` is alpha software. Security fixes target the latest published release and
the current `main` branch.

## Scope

Security reports may cover:

- the `ripr` CLI
- the `ripr` library API
- the experimental LSP sidecar
- the VS Code extension
- release binaries, VSIX packages, manifests, and download/update paths
- repository automation that handles publish or release credentials

Static exposure findings are product output, not security claims. Report a
finding as a vulnerability only when it creates a concrete security risk in the
tool, packaging, distribution, or release process.

## Analyzing A Repository You Do Not Trust

`ripr` reads a repository through `git`, and `git` can run programs that the
repository's own configuration names. A `git clone` never copies
`.git/config`, but an extracted archive or a nested repository inside a
checkout can carry one. Every `ripr` git call therefore:

- disables `core.fsmonitor`, points `core.hooksPath` at the null device and
  takes no optional index lock, so no file-system monitor or hook runs;
- passes `--no-ext-diff --no-textconv` on the diffs it parses, so no external
  diff or text-conversion program runs;
- sets `log.showSignature=false`, so `gpg.program` is not run to verify
  signed commits;
- resets every `filter.<driver>.clean`, `.smudge`, `.process` and `.required`
  key that the repository (or worktree) configuration defines to the value
  your own system or global git configuration gives it, or to nothing. A
  driver you installed yourself, such as Git LFS from `git lfs install`,
  keeps working. A driver defined only in the repository, such as one from
  `git lfs install --local`, does not run during `ripr` reads, so files it
  filters can show as modified to `ripr`. A repository-defined driver whose
  name contains `=` cannot be reset, and `ripr` refuses to run git there;
- passes `--ignore-submodules=dirty` on every diff and status call, so git
  compares a populated submodule's checked-out commit with the recorded
  pointer but never examines that submodule's working tree. No git process
  runs inside a submodule, so programs that the submodule's own configuration
  names (such as its filter drivers) do not run. Uncommitted edits inside a
  submodule are therefore not reported; a moved submodule commit still is.

Not covered: programs that the analyzed project's own build or test tooling
runs when you ask `ripr` to execute tests are outside these pins. Treat a
tree from an untrusted source accordingly.

## Reporting A Vulnerability

Use GitHub private vulnerability reporting for this repository when available.
If that is unavailable, contact the maintainer privately before public
disclosure.

Please include:

- affected version or commit
- affected surface
- reproduction steps
- expected and observed behavior
- impact assessment
- any known workaround

Do not include live credentials, private customer data, or exploit payloads that
are not needed to understand the issue.

## Response Expectations

The maintainer will triage credible reports as soon as practical, coordinate on
scope and impact, and publish fixes or mitigations through the normal release
channels.

Please do not publicly disclose a vulnerability until a fix or mitigation is
available, or until coordinated disclosure timing has been agreed.
