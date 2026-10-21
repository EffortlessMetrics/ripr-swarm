# Fixture: effect_assertion_in_false_block

Spec: RIPR-SPEC-0197

## Given

`process_order` now forwards `order_id` to `notifier.send`. The only related test
calls it, then holds a strong `assert!(matches!(..))` inside a literal `if false` block. The assertion never runs, so the test cannot notice
a wrong payload.

## When

`ripr check --diff diff.patch` classifies the `side_effect` and `call_deletion`
probes.

## Then

Neither effect probe may be credited from the assertion that never runs:
classification stays `reachable_unrevealed`, observe and discriminate are `no`,
and the oracle is `unknown`/`none`. The reachable controls are
`effect_assertion_runs_compact_mixed` and `effect_assertion_std_qualified_live`,
where a same-spelling assertion runs and the probes keep their credit (#8927).

## Must Not

The never-run assertion must not raise `observe` or `discriminate`, set an oracle
strength, or promote either effect probe. Reachability of the changed effect
stays `yes`; the finding is `reachable_unrevealed`, not `no_static_path`.
