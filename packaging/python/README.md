# ripr

`ripr` finds static mutation-exposure gaps before expensive mutation testing.
The **PyPI distribution name is `ripr-rs`**, while the installed executable and
product name remain `ripr`.

> **Qualification status:** this package adapter is under no-publish rehearsal.
> A local wheel does not mean that a public PyPI release exists.
> Use published installation commands only after a release has been independently verified from PyPI.

## Intended installation

Persistent tool installation:

```console
uv tool install ripr-rs
ripr --version
ripr check
```

One-shot execution with the distribution named explicitly:

```console
uvx --from ripr-rs ripr check
```

The explicit `--from ripr-rs` matters because another PyPI project uses the
unscoped distribution name `ripr` and also installs a command with that name.
Use an isolated tool environment rather than replacing an unrelated ambient
executable.

For a reviewed local wheelhouse during qualification:

Create a fresh tool environment with Python 3.10 or newer, then use that
environment's interpreter and executable explicitly. Activation is optional;
installing with a selected Python does not put its `ripr` on PATH.
Keep this tool environment separate from the project's test environment.

On Linux or macOS:

```console
python3 -m venv ".ripr-tool"
".ripr-tool/bin/python" -m pip install --no-index --only-binary=:all: --find-links "./wheelhouse" ripr-rs
".ripr-tool/bin/ripr" --version
```

On Windows PowerShell:

```powershell
py -3 -m venv ".ripr-tool"
& ".\.ripr-tool\Scripts\python.exe" -m pip install --no-index --only-binary=:all: --find-links "./wheelhouse" ripr-rs
& ".\.ripr-tool\Scripts\ripr.exe" --version
```

Choose an unused tool-directory name and keep both paths quoted when replacing
it or the wheelhouse path, including paths with spaces or non-ASCII characters.
Use the same explicit executable path for later commands and fresh shells.

To upgrade from a newly reviewed wheelhouse, repeat the same environment's pip
command with `--upgrade`; to replace installed files from that wheelhouse, use
`--force-reinstall` instead. Run its explicit `ripr --version` afterward.
These operations target the tool environment rather than the project's Python.

If pip reports no matching distribution, check that the local wheelhouse contains
a wheel qualified for this OS/architecture and Python version. `--no-index` and
`--only-binary=:all:` keep this route offline and wheel-only; obtaining a compatible
reviewed wheel is a separate step. If the explicit `ripr` path is missing, inspect
the installation failure and rerun pip with the same tool interpreter. A bare
`ripr` can select another installation and does not diagnose this environment.

## Package contract

- The wheel contains the native `ripr` executable; it does not compile Rust on
  the consumer machine and does not download a binary at installation or first
  run.
- This is a CLI distribution. It does not provide `import ripr` or
  `python -m ripr`.
- Initial delivery is wheel-only. There is no implicit source-distribution
  fallback that compiles the project when a compatible wheel is unavailable.
- Platform and minimum-OS compatibility claims apply only to wheel files whose
  installed payloads completed the corresponding qualification.
- Packaging does not change language activation, analysis behavior, output
  schemas, or the product's advisory claim boundary.

Source, documentation, and issue tracking remain under the
[`EffortlessMetrics/ripr`](https://github.com/EffortlessMetrics/ripr)
repository.
