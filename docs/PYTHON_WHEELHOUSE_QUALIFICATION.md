# Python wheelhouse qualification

`Python Wheelhouse Qualification` is an Actions-only rehearsal lane for
[#4631](https://github.com/EffortlessMetrics/ripr-swarm/issues/4631). It
installs candidate wheels from a local wheelhouse with pip and uv, then
evaluates a fail-closed aggregate. It is not a publisher and does not replace
[#4490](https://github.com/EffortlessMetrics/ripr-swarm/issues/4490),
[#4626](https://github.com/EffortlessMetrics/ripr-swarm/issues/4626), or the
generic cross-channel DTO in
[#4630](https://github.com/EffortlessMetrics/ripr-swarm/issues/4630).

## Invocation

Dispatch `.github/workflows/python-wheelhouse-qualification.yml` with:

- `candidate_sha`: the full 40-character immutable candidate commit SHA.

Three jobs run. `stage` checks out the SHA with `contents: read` and
`persist-credentials: false`, builds exactly one linux/x86_64 wheel with the
pinned maturin, downloads the pinned offline pip and uv tooling, and records
the wheel digest and the digest of the `ripr` script inside it. `install` runs
once per client (pip, uv) on a runner that has no checkout, installs from the
staged wheelhouse only, runs the installed `ripr` on a Python fixture, and
writes one row. `aggregate` checks the SHA out again, assembles the identity
and rows, and runs the gate below. Receipts are retained as an Actions artifact
for 5 days.

Isolation is measured, not assumed:

- Network: the install and the journey run in a private network namespace
  (`unshare --net`), and the journey fails if `https://pypi.org` is reachable
  inside it.
- Source checkout: the consumer job never checks out the repository; the row
  records whether any `Cargo.toml` exists under the workspace.
- Cargo and ambient `ripr`: probed with the same minimal `PATH` the consumer
  uses, before the install.
- Planted binary: a wrong-path `ripr` is placed on `PATH` behind the install and
  must be what runs without it; the installed one must resolve first, match the
  wheel's script digest, and report the candidate version and SHA.
- Subjects: `subject_count` is the number of findings the installed binary
  produced on the fixture; a run that analyzes nothing cannot pass.

## Gate

`cargo xtask qualify-python-wheelhouse --receipts <file-or-dir>` is the
authority. A required linux/x86_64 pip or uv row can pass only when it records
exact wheel and payload digests, a matching wheel filename across required
passed rows, a nonzero subject count, no Cargo/source checkout/ambient `ripr`
escape, a detected planted wrong-PATH binary, and network disabled after
staging. Failed, `not_run`, unsupported, missing, zero-subject, or mismatched
rows cannot satisfy the aggregate.

## Claim boundary

A passing run would establish that the named candidate's selected linux/x86_64
wheel installed from a local wheelhouse through pip and uv with the isolation
controls above. It does not establish PyPI publication, a compatibility floor,
a support-tier promotion, or the five-target matrix. A green pip `--no-index`
install is not network isolation on its own; the namespace probe is the
evidence.
