# Fixture: python_empty_sink_module_comparison

Spec: RIPR-SPEC-0238

## Given

The test imports unchanged y as module. Python parses 0 <module>(1) as a chained comparison with zero Call nodes. The existing lexical relation reads syntactic_call, but this is no observation of changed x. Current x1 and mutant x9 both pass. Keep the changed module finding weakly_exposed/orthogonal with strong_oracle_observes_different_sink; this fixture does not redesign relation matching.

The Python preview configuration, current production bytes, test assertions and
Git-derived diff are copied without rewriting from independently authored native
controls for #7526. A fresh Python process invoked the exact test function; its
assertions and import values were observed before and after a non-equivalent
mutant. Ordinary module import links remain conservative controls elsewhere.

## When

`ripr check` analyzes the real diff against these current Python sources.

## Then

The test imports unchanged y as module. Python parses 0 <module>(1) as a chained comparison with zero Call nodes. The existing lexical relation reads syntactic_call, but this is no observation of changed x. Current x1 and mutant x9 both pass. Keep the changed module finding weakly_exposed/orthogonal with strong_oracle_observes_different_sink; this fixture does not redesign relation matching.

Runtime rejection is an independent control, not a static mutation outcome or
proof of test adequacy. No new owner identity or shell command is invented.

## Must Not

- Drop the changed finding or replace the original source, assertion, import or diff identity.
- Credit an unrelated assertion merely because no useful sink token remains.
- Downgrade the genuine short-function output control by stopping before its existing credit branch.
- Claim broader module-read, relation-scanner, runtime mutation or test-adequacy support.
