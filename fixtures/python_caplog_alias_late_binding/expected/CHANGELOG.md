# Golden Output Changes

## Pending — python_caplog_alias_late_binding (1)

Reason:
RIPR-SPEC-0238 #10595: native old/new tests raise UnboundLocalError before the snapshot exists; retain one static_unknown/unsupported_syntax finding, never false exposed credit.

Command:
`cargo xtask goldens bless python_caplog_alias_late_binding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_caplog_alias_late_binding (2)

Reason:
RIPR-SPEC-0238 #10595: retain the independently checked one-finding static_unknown/unsupported_syntax result on the full human surface required by the honesty corpus. Native original/changed masking and unbound controls establish the refusal.

Command:
`cargo xtask goldens bless python_caplog_alias_late_binding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_caplog_alias_late_binding (3)

Reason:
RIPR-SPEC-0238 rule 22: refresh expected human/human-full rendering to main's current summary contract (probe->changed behavior, unknown->unclear wording) after the 69321269d5 merge; check.json classification byte-identical, static_unknown unchanged

Command:
`cargo xtask goldens bless python_caplog_alias_late_binding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
