# Fixture: guard_operand_dropped_agreement

Spec: RIPR-SPEC-0197

## Given

`is_pow2` returned `n != 0 && n & (n - 1) == 0` and the diff drops the
`n != 0` guard. The related test pins `is_pow2(4)` to `true` and `is_pow2(6)`
to `false`.

## When

The exact diff is analyzed.

## Then

The return-value finding is not `exposed`: the dropped `n != 0` is `true` at
both pinned inputs, so both versions return the same value there and neither
assertion can notice the edit (#8361, short-circuit tails item 5).

## Must Not

- Promote the change to `exposed` from pins whose literal inputs leave the
  dropped `&&` operand at `true`.
