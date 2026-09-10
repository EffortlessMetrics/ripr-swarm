# Golden Output Changes

## Pending — constant_declaration_probe_reconciliation (1)

Reason:
RIPR-SPEC-0046: #3719 reconciliation — constant declarations classify static_unknown (no call_deletion/field_construction) and the canonical alignment records the config_or_policy_constant limitation with unknown flow

Command:
`cargo xtask goldens bless constant_declaration_probe_reconciliation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
