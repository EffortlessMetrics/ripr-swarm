# Fixture Corpus: actionable-gap-outcomes-corpus

Spec: RIPR-SPEC-0031

Related: RIPR-SPEC-0057

## Given

`cargo xtask actionable-gap-outcomes` consumes actionable canonical gap packets
plus optional receipt and targeted-test outcome artifacts. Raw findings remain
supporting evidence in the packet source and do not become independent outcome
rows.

## When

The outcome report joins packets to receipts and evidence movement.

## Then

Each case pins one bounded outcome state:

- not attempted;
- receipt present without movement;
- evidence improved;
- evidence unchanged;
- evidence regressed;
- resolved;
- attempted without a matching receipt.

## Must Not

Fixtures must not imply repair execution, provider calls, generated tests,
mutation execution, public badge changes, PR/CI rendering changes, autonomous
merges, or raw-finding consumption.
