# Fixture: returned_comparison_rival_import

Spec: RIPR-SPEC-0108

Owner contract: RIPR-SPEC-0197
Issue: #10046

## Given

The changed owner moves `x > 100` to `x > 101`. Its tests call it at 50
and 500. A separate test imports `rival::thr` and calls that rival at 101. Another
test locally shadows the imported owner with a function pointer and calls
that unrelated callable at 101.

## When

`cargo xtask fixtures returned_comparison_rival_import`

## Then

The rival's argument supplies no owner equality observation. Both comparison
findings withhold the boundary claim as `infection_unknown`; the return-value
finding never invents a missing equality-boundary discriminator.

## Must Not

Credit the rival's 101 as an observed owner boundary or describe withheld
call identity as a known missing test input. The integration tests retain
owner-import, workspace-sibling and computed-input positive/negative controls.
