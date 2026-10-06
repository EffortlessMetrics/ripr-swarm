# Golden Output Changes

## Pending — rust_proptest_quickcheck_tests (1)

Reason:
RIPR-SPEC-0001: index proptest! and quickcheck! tests so the fixture crate relates gate_threshold

Command:
`cargo xtask goldens bless rust_proptest_quickcheck_tests --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_proptest_quickcheck_tests (2)

Reason:
RIPR-SPEC-0001: quarantine unresolved property macro promotion and honor producer-owned typed limitations in human triage; ordinary discriminator controls remain unchanged

Command:
`cargo xtask goldens bless rust_proptest_quickcheck_tests --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## #5051 review correction

Opaque property-only call arguments and lexical fallback bodies provide no reach, infection or propagation proof. Known unrelated-package mentions cannot suppress a real gap.

## Pending — rust_proptest_quickcheck_tests (3)

Reason:
RIPR-SPEC-0122: check names the one-step ripr agent stub route for Rust value gaps (#5355)

Command:
`cargo xtask goldens bless rust_proptest_quickcheck_tests --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_proptest_quickcheck_tests (4)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless rust_proptest_quickcheck_tests --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_proptest_quickcheck_tests (5)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.

Command:
`cargo xtask goldens bless rust_proptest_quickcheck_tests --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_proptest_quickcheck_tests (6)

Reason:
RIPR-SPEC-0116: the check drill-in binds --root to the resolved repository (#3948); goldens carry the <cwd>/ placeholder

Command:
`cargo xtask goldens bless rust_proptest_quickcheck_tests --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
