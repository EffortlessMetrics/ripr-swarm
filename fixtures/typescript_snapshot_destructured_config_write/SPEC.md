# Fixture: typescript_snapshot_destructured_config_write

Spec: RIPR-SPEC-0258
Spec: RIPR-SPEC-0108

## Given

A single direct snapshot assertion and committed `price 1` entry. The Jest
config writes a variable-computed property through an array assignment target.
The key evaluates to `snapshotResolver`, so the exported config changes the
snapshot identity even though the override name is not a literal config key.

## When

Analyze the return change in `diff.patch` with the TypeScript adapter.

## Then

The finding is `weakly_exposed` with a `snapshot` / medium oracle. The honesty
corpus independently pins that result. The existing
`typescript_snapshot_single_call_control` is the plain-config strong control;
the focused config-admission tests cover object/nested/default/rest targets,
TypeScript wrappers, loop destinations and unrelated destructuring.

## Must Not

- Preserve strong snapshot credit by ignoring destructuring destinations.
- Re-bless an exposed result as the expected contract.
