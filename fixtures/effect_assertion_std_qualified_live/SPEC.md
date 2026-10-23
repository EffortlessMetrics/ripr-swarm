# Fixture: effect_assertion_std_qualified_live

Spec: RIPR-SPEC-0197

## Given

`process_order` now forwards `order_id` to `notifier.send(order_id);` (line 18, an
aligned statement followed by a trailing `true`). The only related test calls it, then holds an unconditional `std::assert!` equality check (no nested operand macro). The standard-rooted assertion runs, so the test does notice a wrong payload.

## When

`ripr check --diff diff.patch` classifies the `side_effect` and `call_deletion`
probes.

## Then

Reachable-oracle preservation: both effect probes stay `weakly_exposed` (observe `yes`, discriminate `weak`, oracle `mock_expectation`/`medium`): a verified standard-rooted live assertion keeps its credit (#8927).

## Must Not

The live assertion must not be refused as unexecuted. The dead controls
(`effect_assertion_after_return`, `effect_assertion_in_false_block`,
`effect_assertion_uncalled_closure`) must stay `reachable_unrevealed`.
