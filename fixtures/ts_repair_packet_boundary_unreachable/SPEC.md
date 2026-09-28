# Fixture: ts_repair_packet_boundary_unreachable

Spec: RIPR-SPEC-0087

## Given

A single-package TypeScript workspace where `login` has a boundary condition
change (`>` → `>=` on `user.length`) and an oracle-eligible related test with:

- A direct import-aware call relation (`import { login } from '../src/auth'`)
- A weak relational oracle (`expect(login('alice')).toBeGreaterThan(4)`) with a
  concrete literal expected value (`4`) and `has_dynamic_matcher_arg == false`
- A discoverable `package.json` with `jest` in `devDependencies` and `scripts.test`
- `package-lock.json` confirming npm runner
- A named missing discriminator (`user.length == 3`) from the boundary expression
- An observed oracle call input `'alice'` whose static string length (5) provably
  does NOT satisfy that discriminator boundary (issue #4105)

## When

```bash
ripr check \
  --root fixtures/ts_repair_packet_boundary_unreachable/input \
  --diff fixtures/ts_repair_packet_boundary_unreachable/diff.patch
```

## Then

The TypeScript preview adapter:

- Classifies the finding as `WeaklyExposed` (oracle strength is Weak, not Strong)
- Projects a `GapRecord` whose `assertion_shape` is the boundary placeholder
  `expect(login(/* boundary input for user.length == 3 */)).toBe(expected)` — it must NOT
  reuse the observed input `login('alice')`, which statically cannot reach the
  named boundary (G-G, #4105)
- Adds a stop condition forbidding reuse of the observed call input
- Keeps the packet delegatable: the boundary `user.length == 3` is concrete, so
  the placeholder names exactly what the new assertion must hit. The observed
  input missing the boundary is the gap the repair closes, not a reason to
  refuse it (the Rust `boundary input where …` shape follows the same contract)
- Keeps `authority_boundary: preview_advisory_only` (TypeScript stays preview)

## Must Not

- Present the observed call input (`login('alice')`) as the assertion shape
  for the missing discriminator — an agent following it verbatim would
  duplicate a non-discriminating assertion
- Emit `typescript_repair_packet` without the shared validator returning `Ok(())`
- Change `schema_version`
- Add new public symbols
- Execute runtime code, call providers, or generate tests

An unresolved named-constant boundary (`amount == DISCOUNT_THRESHOLD`) still
fails closed; see `ts_repair_packet_boundary_constant_unresolved`.
