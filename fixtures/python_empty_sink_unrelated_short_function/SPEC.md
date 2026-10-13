# Fixture: python_empty_sink_unrelated_short_function

Spec: RIPR-SPEC-0238

## Given

A genuine call discards the result of the one-letter function f, then the strong assertion observes only unchanged y. Both current return1 and mutant return9 pass. The changed finding must remain weakly_exposed/orthogonal, with strong_oracle_observes_different_sink.

The Python preview configuration, current production bytes, test assertions and
Git-derived diff are copied without rewriting from independently authored native
controls for #7526. A fresh Python process invoked the exact test function; its
assertions and import values were observed before and after a non-equivalent
mutant. Ordinary module import links remain conservative controls elsewhere.

## When

`ripr check` analyzes the real diff against these current Python sources.

## Then

A genuine call discards the result of the one-letter function f, then the strong assertion observes only unchanged y. Both current return1 and mutant return9 pass. The changed finding must remain weakly_exposed/orthogonal, with strong_oracle_observes_different_sink.

Runtime rejection is an independent control, not a static mutation outcome or
proof of test adequacy. No new owner identity or shell command is invented.

## Must Not

- Drop the changed finding or replace the original source, assertion, import or diff identity.
- Credit an unrelated assertion merely because no useful sink token remains.
- Downgrade the genuine short-function output control by stopping before its existing credit branch.
- Claim broader module-read, relation-scanner, runtime mutation or test-adequacy support.
