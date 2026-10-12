# Expected output history

## #5051

Initial public CLI baseline for the governed package/fallback admission control.

## #5051 review correction

Opaque property-only call arguments and lexical fallback bodies provide no reach, infection or propagation proof. Known unrelated-package mentions cannot suppress a real gap.

## Pending — property_macro_lexical_fallback (1)

Reason:
RIPR-SPEC-0122: check names the one-step ripr agent stub route for Rust value gaps (#5355)

Command:
`cargo xtask goldens bless property_macro_lexical_fallback --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — property_macro_lexical_fallback (2)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless property_macro_lexical_fallback --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — property_macro_lexical_fallback (3)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.

Command:
`cargo xtask goldens bless property_macro_lexical_fallback --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — property_macro_lexical_fallback (4)

Reason:
RIPR-SPEC-0116: the check drill-in binds --root to the resolved repository (#3948); goldens carry the <cwd>/ placeholder

Command:
`cargo xtask goldens bless property_macro_lexical_fallback --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — property_macro_lexical_fallback (5)

Reason:
RIPR-SPEC-0122: a predicate's before is cut to the same span as its after (#6995)

Command:
`cargo xtask goldens bless property_macro_lexical_fallback --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — property_macro_lexical_fallback (6)

Reason:
RIPR-SPEC-0122 #5471: stub route printed only when the resolver yields a stub, with --kind; refusal or nothing otherwise (merge re-bless)
RIPR-SPEC-0117: rust_macro_reach_unresolved description no longer claims the class stays no_static_path (#7071)

Command:
`cargo xtask goldens bless property_macro_lexical_fallback --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
