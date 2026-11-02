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

## Pending — python_caplog_alias_assertion_store (4)

Reason:
RIPR-SPEC-0122: no-gap-ID refusal wording re-blessed now that the goldens gate compares it verbatim (#12003)

Command:
`cargo xtask goldens bless python_caplog_alias_assertion_store --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_caplog_alias_assertion_store (5)

Reason:
RIPR-SPEC-0182: Python finding ids move to the versioned v1- namespace (#11368); classification and finding count unchanged; fixture diff and input made consistent so source context is verifiable

Command:
`cargo xtask goldens bless python_caplog_alias_assertion_store --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_caplog_alias_assertion_store (6)

Reason:
RIPR-SPEC-0118 / #10692: plain wording for seam and discriminator in default check and repair-start text; verdicts, identity and JSON fields unchanged

Command:
`cargo xtask goldens bless python_caplog_alias_assertion_store --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_caplog_alias_assertion_store (7)

Reason:
RIPR-SPEC-0122: explain missing input or assertion honestly

Command:
`cargo xtask goldens bless python_caplog_alias_assertion_store --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
