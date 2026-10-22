# Golden Output Changes

## Pending — owner_return_pin_pointer_mask_unexercised (1)

Reason:
RIPR-SPEC-0197 / #9478: retain the confirmed typed-pointer masked-input counterexample; 0x1234 leaves bit 0x80 clear so neutral 0xff and narrower 0x7f both return 0x34. Matched native controls execute one test each; all-bit controls kill the narrower mask. No exposure promotion is allowed.

Command:
`cargo xtask goldens bless owner_return_pin_pointer_mask_unexercised --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
