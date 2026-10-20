# RIPR-SPEC-0282: Python framework scan is bounded, deterministic, and string-aware

Status: proposed

Owner: product / analysis

Created: 2026-10-19

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #8706 (doctor framework scan flips run to run and starves on cap)
- #8707 (docstring example text credits pytest/unittest)
- #8706/#8707 review follow-ups on PR #8901 (comment and string-literal
  quote delimiters; test-named directories)

Linked PRs:

- #8901

Support-tier impact:

- No tier change. The scan feeds the doctor/repo-input framework facts;
  its admission, ordering, cap, and string-awareness rules are internal
  evidence hygiene, not a new oracle kind, strength, class, relation
  reason or limitation kind.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `.ripr/traceability.toml`.
- No schema version bump. No new wire shape: `detect_python_test_frameworks`
  still returns the same ordered framework set.

## Problem

`detect_python_test_frameworks` admits per-directory candidates for a
bounded import-evidence read. Three independent defects made its answer
wrong or unstable:

1. The candidate stream was the raw `read_dir` enumeration, capped before
   filtering. 64 non-test entries (or 64 directories named `test_*.py`,
   which pass the name filter but can never yield evidence) starved the
   scan of every real test file, and the surviving set depended on
   unspecified filesystem enumeration order — the same repository could
   report a different framework set between runs and machines (#8706).
2. Framework imports were matched against raw file text. A module
   docstring or usage-example string that merely SHOWS `import pytest` /
   `import unittest` was credited as framework evidence. `tests.py` /
   `test.py` modules are admitted by file name alone (#7706), so a helper
   module's example text reported a confirmed framework (#8707).
3. The first string-awareness fix toggled the triple-quote state on every
   unescaped `"""` / `'''`, including ones inside `#` comments and inside
   ordinary single-line strings. A line like `# use """ for docstrings` or
   `x = "'''"` opened a phantom string that swallowed later real imports —
   a false negative, the opposite of the #8707 goal (PR #8901 review).

## Behavior

### Rule 1: admission and cap

Within each scanned directory (workspace root, `tests/`, `test/`, and
each immediate app directory one level down, that list itself sorted and
capped at 64):

- Only regular files count. Directories — including directories named
  `test_*.py` — never count toward the per-directory cap.
- A candidate file is admitted when its name ends in `.py` and the shared
  [`is_test_file`] predicate accepts its path (root-level or under
  `tests/` / `test/`, Django-conventional `tests.py` / `test.py` modules
  and `*_test.py` siblings).
- Admitted candidates are sorted by file name before the cap, and at most
  `PYTHON_FRAMEWORK_SCAN_DIR_ENTRY_CAP` (64) are read per directory. The
  bounded answer is therefore deterministic across filesystems.

### Rule 2: bounded evidence read

Each admitted candidate is read for at most
`PYTHON_FRAMEWORK_EVIDENCE_PREFIX_BYTES` (4096) bytes; a cut final line is
dropped rather than classified, so a truncated `import unittest` prefix
cannot complete into a false match.

### Rule 3: string-aware evidence

Framework evidence is matched only against code lines — lines outside any
triple-quoted string literal:

- Outside a string, scanning a line stops at an unquoted `#` (the rest is
  a comment) and skips ordinary single-line string contents to their
  matching unescaped quote, so neither can contribute a phantom
  triple-quote delimiter.
- Inside a triple-quoted string, only the matching closing delimiter ends
  the string; `#` and the other delimiter are content. Code resumes on the
  same line after a closing delimiter.
- Escaped quotes never toggle state.

This is a bounded heuristic, not a full tokenization: single-line strings
cannot place an import at line start, and the import matchers already
reject lookalike tokens and commented-out imports.

### Rule 4: detection outcomes

- pytest: config markers (`pytest.ini`, `conftest.py`, pytest sections in
  `pyproject.toml` / `setup.cfg` / `tox.ini`) or a code-level pytest
  import in an admitted candidate.
- unittest: a code-level unittest import in an admitted candidate (no
  config exists by design).
- Both may be reported; pytest sorts first. An empty result is fail-closed:
  callers report "not detected", never guess.

## Required Evidence

- A repository with 64 non-test entries or 64 test-named directories ahead
  of the only real test file reports the framework from the real file,
  deterministically across runs.
- A `tests.py` whose docstring shows `import unittest` reports no
  framework; the same file with the import at code level reports unittest.
- `# use """ for docstrings` followed by `import pytest` reports pytest;
  `x = "'''"` followed by `import unittest` reports unittest; either quote
  form alone in a comment or string reports nothing.

## Non-Goals

- Full Python tokenization (f-strings, line continuations, raw-string
  prefixes are not modeled).
- A `scan_capped` disclosure field plumbing the cap through doctor JSON —
  considered in #8706 and judged out of scope.
- Changing adapter-level framework detection; this spec covers the
  doctor/repo-input scan only.

## Acceptance Examples

1. `tests/` holds `test_00.py` … `test_63.py` as directories and
   `test_z.py` contains `import pytest`: pytest is reported.
2. `tests/` holds 64 `a-fixture-*.json` files and `test_core.py` contains
   `import pytest`: pytest is reported, twice-run answers are identical.
3. `blog/tests.py` is `"""Blog helpers.\n\nUsage example:\n    import unittest\n"""`:
   no framework is reported.
4. `test_pricing.py` is `"""Pricing tests."""\nimport unittest\n`:
   unittest is reported.
5. `test_comment.py` is `# use """ for docstrings\nimport pytest\n`:
   pytest is reported (the comment delimiter is not a string).
6. `test_literal.py` is `x = "'''"\nimport unittest\n`: unittest is
   reported (the literal is content, not a delimiter).
7. `test_note.py` is `# """ example\n`: no framework is reported.

## Test Mapping

- `crates/ripr/src/analysis/language/python/tests.rs`: examples 1 and 2
  (`detect_python_test_frameworks_scan_is_starvation_free_and_deterministic`,
  `detect_python_test_frameworks_test_named_directories_do_not_starve_the_scan`),
  example 3 (`detect_python_test_frameworks_ignores_imports_inside_string_literals`),
  examples 4 to 7 (`detect_python_test_frameworks_ignores_comment_and_string_quote_delimiters`,
  including the example-4 positive control).

## Implementation Mapping

- `crates/ripr/src/analysis/language/python/source_facts.rs`:
  `detect_python_test_frameworks`, `python_framework_scan_dirs`,
  `code_lines_outside_triple_quoted_strings`,
  `update_triple_quote_state`, `find_triple_open`, `find_closing_triple`,
  `read_framework_evidence_prefix`, `drop_truncated_final_line`,
  `PYTHON_FRAMEWORK_SCAN_DIR_ENTRY_CAP`,
  `PYTHON_FRAMEWORK_EVIDENCE_PREFIX_BYTES`.

## Metrics

- `unit_test_pass_rate`: the framework-scan behavior tests above are
  required Rust tests; a failure blocks merge.
