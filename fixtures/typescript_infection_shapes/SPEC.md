# Fixture: typescript_infection_shapes

Spec: RIPR-SPEC-0027

## Given

A TypeScript pricing module changes several lines in one diff, each a change
shape for which the preview adapter has no value-level infection model
(issue #10704):

- a literal return value (`return 5;` to `return 6;`),
- a `??` fallback literal (`?? 10` to `?? 20`),
- a default parameter (`loud = false` to `loud = true`),
- an arithmetic no-op (an exact trailing ` + 0`, ` - 0`, ` * 1` or ` / 1`, here `a / b` to `a / b + 0`; never claimed behavior-neutral, so a strong oracle keeps `exposed`),
- a comment-only edit (`return price;` to `return price; // fallback`), which since #10706 emits no probe at all (see `ts_noop_trailing_comment`).

A second module changes two lines under an `if` arm. One related test passes
inputs that never enter the arm (`safeDiv(4, 2)` against `b === 0`); the other
passes an input that does (`floorAt(-1, 0)` against `x < lo`). Both tests use a
weak `toBeGreaterThan` oracle.

## When

```bash
ripr check \
  --root fixtures/typescript_infection_shapes/input \
  --diff fixtures/typescript_infection_shapes/diff.patch \
  --mode draft
```

## Then

- every unmodeled shape under a weak oracle reads `infection_unknown` with infection `unknown`, a
  `typescript_infection: unknown (<shape>)` evidence line, the
  `infection_evidence_unknown` stop reason, no missing discriminator, no
  "assertion too weak" row and `repair_packet_ready = false`;
- the comment-only edit emits no finding under any oracle (#10706); the identity no-op under a strong oracle keeps `exposed`, because the adapter cannot prove `+ 0` neutral for strings or BigInt;
- the arm no test input enters reads `reachable_unrevealed` (no stop reason: it is a concrete gap, not an unknown) with infection
  `no` (`typescript_infection: no (branch_not_entered)`), never
  `weakly_exposed` with an "assertion too weak" explanation;
- the arm a test input does enter stays `weakly_exposed`.

## Remaining (not pinned by this fixture)

The #10704 acceptance is only partly met. Not pinned here: the `>=` / `>`
predicate and ternary infection state, the template literal edit, `let t = 0`
to `1`, `reduce((a, b) => a + b, 0)` to `a - b`, `throw` message edits and
single numeric operands such as `Math.floor(price * 0.9)`. They keep their
previous verdicts and are tracked in the follow-up issue.

## Must Not

- Turn an unmodeled shape into an actionable or delegatable repair packet.
- Claim a runtime mutation outcome from static evidence.
- Move the actionability decision out of the shared packet validator.
