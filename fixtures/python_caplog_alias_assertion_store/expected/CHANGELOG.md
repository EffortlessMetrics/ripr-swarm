# Golden Output Changes

## Pending — python_caplog_alias_assertion_store (1)

Reason:
RIPR-SPEC-0238 #10595: native walrus mutation inside the assertion masks both old/new messages; one static_unknown/unsupported_syntax finding independently guards against false exposure credit.

Command:
`cargo xtask goldens bless python_caplog_alias_assertion_store --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_caplog_alias_assertion_store (2)

Reason:
RIPR-SPEC-0238 #10595: retain the independently checked one-finding static_unknown/unsupported_syntax result on the full human surface required by the honesty corpus. Native original/changed masking and unbound controls establish the refusal.

Command:
`cargo xtask goldens bless python_caplog_alias_assertion_store --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_caplog_alias_assertion_store (3)

Reason:
RIPR-SPEC-0238: refresh human-full.txt to current summary wording after rebase onto main (#10595)

Command:
`cargo xtask goldens bless python_caplog_alias_assertion_store --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending - python_caplog_alias_assertion_store (4)

Reason:
#10697: plain wording for discriminator and seam in default human output; mirror the shared evidence/legend/no-seam rewording and reword TS preview discriminator labels to observed/safe checks

Command:
`cargo xtask goldens bless python_caplog_alias_assertion_store --reason "..."`

Updated:
- `expected/human.txt`
