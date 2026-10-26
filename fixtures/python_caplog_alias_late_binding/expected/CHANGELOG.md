# Golden Output Changes

## Pending — python_caplog_alias_late_binding (1)

Reason:
RIPR-SPEC-0238 #10595: native old/new tests raise UnboundLocalError before the snapshot exists; retain one static_unknown/unsupported_syntax finding, never false exposed credit.

Command:
`cargo xtask goldens bless python_caplog_alias_late_binding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
