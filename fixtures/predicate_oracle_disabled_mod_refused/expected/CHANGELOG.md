# Golden Output Changes

## Pending — predicate_oracle_disabled_mod_refused (1)

Reason:
RIPR-SPEC-0197: always-disabled cfg(any()) keeps the module-declaration refusal and no oracle credit (#14537)

Command:
`cargo xtask goldens bless predicate_oracle_disabled_mod_refused --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — predicate_oracle_disabled_mod_refused (2)

Reason:
RIPR-SPEC-0122: the top-gap Safe next action points at the Next step the digest prints (#14173)

Command:
`cargo xtask goldens bless predicate_oracle_disabled_mod_refused --reason "..."`

Updated:
- `expected/human.txt`

## Pending — predicate_oracle_disabled_mod_refused (3)

Reason:
RIPR-SPEC-0153: a mod behind #[cfg(any())] mounts nothing, rustc never compiles its file (#15877)

Command:
`cargo xtask goldens bless predicate_oracle_disabled_mod_refused --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
