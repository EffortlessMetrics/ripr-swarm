# Golden Output Changes

## Pending — python_transitive_reach_positive (1)

Reason:
RIPR-SPEC-0201: initial Python same-class transitive-reach positive fixture

Command:
`cargo xtask goldens bless python_transitive_reach_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_transitive_reach_positive (2)

Reason:
RIPR-SPEC-0201: conflict repair against main. The fixture diffs declared @@ -10,7 +10,7 @@ but carried 6 old / 6 new body lines; main's stricter hunk-span validation (e0cfe2186, #4439) discloses that as malformed_diff and flips the outcome to unsupported_input. Headers now declare the honest -10,6 +10,6 span (changed lines and coordinates unchanged); goldens re-blessed for main's current renderer (summary vocabulary, source_subject, analysis_complete) while static_limit_kind python_transitive_reach_unresolved still attaches.

Command:
`cargo xtask goldens bless python_transitive_reach_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_transitive_reach_positive (3)

Reason:
RIPR-SPEC-0201: conflict repair against main. The fixture diffs declared @@ -10,7 +10,7 @@ but carried 6 old / 6 new body lines; main's stricter hunk-span validation (e0cfe2186, #4439) discloses that as malformed_diff and flips the outcome to unsupported_input. Headers now declare the honest -10,6 +10,6 span (changed lines and coordinates unchanged); goldens re-blessed for main's current renderer (summary vocabulary, source_subject, analysis_complete) while static_limit_kind python_transitive_reach_unresolved still attaches.

Command:
`cargo xtask goldens bless python_transitive_reach_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_transitive_reach_positive (4)

Reason:
RIPR-SPEC-0201: conflict repair against main. The fixture diffs declared @@ -10,7 +10,7 @@ but carried 6 old / 6 new body lines; main's stricter hunk-span validation (e0cfe2186, #4439) discloses that as malformed_diff and flips the outcome to unsupported_input. Headers now declare the honest -10,6 +10,6 span (changed lines and coordinates unchanged); goldens re-blessed for main's current renderer (summary vocabulary, source_subject, analysis_complete) while static_limit_kind python_transitive_reach_unresolved still attaches.

Command:
`cargo xtask goldens bless python_transitive_reach_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_transitive_reach_positive (5)

Reason:
RIPR-SPEC-0201: review repair (CodeRabbit thread PRRT_kwDOSiSx0c6oY-y2). The hunk start was off by one: the first body line return self._get_padding_width(0) is source line 9, not 10, so the changed if-line reported as 13 instead of its true line 12. Headers now declare @@ -9,6 +9,6 @@; goldens regenerated for the corrected coordinates.

Command:
`cargo xtask goldens bless python_transitive_reach_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_transitive_reach_positive (6)

Reason:
Reconcile the human Summary header with RIPR-SPEC-0122 and the shared
renderer merged in #5049 (Refs #5017, #5021). Exact-main output comparison
shows only `1 of 1 finding(s) shown` becoming `1 of 1 finding(s)
unsuppressed`; all remaining human bytes and the complete JSON output are
unchanged. Both fixtures retain `no_static_path` with no related tests;
only the positive fixture retains `python_transitive_reach_unresolved`.

Method:
Manually reconcile the single header word after comparing full generated
human and JSON outputs; verify with `cargo xtask goldens check`.

Updated:
- `expected/human.txt`
