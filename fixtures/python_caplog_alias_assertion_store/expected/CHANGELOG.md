# Golden Output Changes

## Pending — python_caplog_alias_assertion_store (1)

Reason:
RIPR-SPEC-0238 #10595: native walrus mutation inside the assertion masks both old/new messages; one static_unknown/unsupported_syntax finding independently guards against false exposure credit.

Command:
`cargo xtask goldens bless python_caplog_alias_assertion_store --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
