# Fixture: effect_assertion_runs_compact_mixed

Spec: RIPR-SPEC-0197

## Given

`process_order` now forwards `order_id` to `notifier.send(order_id);` (line 18, an
aligned statement followed by a trailing `true`). The only related test calls it, then holds a strong `assert_eq!` on the same line as a later `return;` and an identical second `assert_eq!` (`A; return; A;`). The first assertion runs before the `return`, so the test does notice a wrong payload.

## When

`ripr check --diff diff.patch` classifies the `side_effect` and `call_deletion`
probes.

## Then

Reachable-oracle preservation: both effect probes stay `exposed` (observe `yes`, discriminate `yes`, oracle `exact_value`/`strong`): the dead twin after `return` neither removes nor adds credit (#8927).

## Must Not

The live assertion must not be refused as unexecuted. The dead controls
(`effect_assertion_after_return`, `effect_assertion_in_false_block`,
`effect_assertion_uncalled_closure`) must stay `reachable_unrevealed`.
