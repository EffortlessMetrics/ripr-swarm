# Golden Output Changes

## Pending — python_caplog_alias_same_line_mask (1)

Reason:
RIPR-SPEC-0238 #10595: native same-line list overwrite passes both message versions; preserve exactly one static_unknown/unsupported_syntax finding instead of false exposed credit.

Command:
`cargo xtask goldens bless python_caplog_alias_same_line_mask --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_caplog_alias_same_line_mask (2)

Reason:
RIPR-SPEC-0238 #10595: retain the independently checked one-finding static_unknown/unsupported_syntax result on the full human surface required by the honesty corpus. Native original/changed masking and unbound controls establish the refusal.

Command:
`cargo xtask goldens bless python_caplog_alias_same_line_mask --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
