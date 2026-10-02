# Blind Journey Contract Fixture Corpus

Spec: RIPR-SPEC-0198

## Given

The 0.11 blind installed-agent journey (#4600, authority #3797) needs a
reusable machine contract before any candidate run begins. The generic #4510
packet knows candidate identity, commands, processes, artifacts and cleanup;
it does not know what the operator was initially told, which information was
prohibited, which actions were product-supported versus private assistance,
whether the prompt leaked the answer key, or whether a transcript is complete
enough for independent audit.

This manifest-only corpus commits typed prompt/answer-key/event/intervention/
receipt packets (`blind_journey_prompt.v1`, `blind_journey_answer_key.v1`,
`blind_journey_receipt.v1`) and the expected validator outcome for every
scenario. `cargo xtask blind-journey-contract` and
`cargo xtask check-fixture-contracts` run each packet through the real
validator in `xtask/src/blind_journey.rs`; a hand-edited expectation cannot
make a wrong packet pass because the validator decides independently.

## When

An offline validator run loads `corpus.json`, recomputes every prompt, answer
key and receipt digest binding, and assesses each packet against the closed
intervention taxonomy, the answer-key comparison, the event-chain ordering
controls, the operator-visible secrecy projection and the terminal result
consistency rules.

## Then

- The clean generic prompt validates `passed_blind_journey`.
- Each mechanical contamination category (issue/PR reference, target test,
  gap id, artifact path, expected command) rejects independently.
- Public docs lookup and ordinary source inspection remain allowed in a
  positive run.
- A private hint, manual artifact plumbing, workspace binary substitution or
  unsafe edit forces its own non-positive terminal result.
- An instrument-only watchdog observation does not change a positive run.
- Either of two eligible product-presented items validates; a quiet neighbor
  fails the answer-key comparison.
- An honest limitation remains an accepted non-positive receipt with the
  complete transcript and exact non-claim.
- Missing predecessors, reordered traces, altered prompt bindings, changed
  answer keys and reviewer labels overriding mechanical findings all reject.
- Equivalent concrete root spellings share one portable semantic identity.
- An unsupported future schema rejects; it cannot aggregate to a clean pass.

## Must Not

- Do not run a candidate, launch a process, or decide a release verdict.
- Do not collect private chain-of-thought; events capture observable actions
  only.
- Do not expose answer-key material in any operator-visible projection.
- Do not count any synthetic validator success as installed usefulness, blind
  qualification, candidate selection or parent acceptance.
- Do not weaken the receipt to claim a positive row for hidden assistance.
