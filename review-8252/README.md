# Canonical #8252 review and acceptance-test handback

Reviewed source: EffortlessMetrics/ripr-swarm PR #8252, exact head
`1ed9c33f6ca1d4aa11a1aedfff2c1f694a81b62d`, merge base
`fabf21c8f8e5a06e61cb3e7c77c96f713dec8a13`. Observed 2026-10-16 UTC.
The published source writer owns the next repair. This packet is evidence and a
test-only adoption patch; it is not a competing source PR or a release artifact.
The PR was observed Ready at this head before this review was published. This
review's disposition is REPAIR_REQUIRED; Ready is not semantic acceptance.

The pre-publication ownership recheck found #8252 after this delegated lane had
formed a local experiment. That experiment remains unpublished at local commit
`d033b16d9db5d0d9624163956a415b9791a2aeb8`. Its tests or results are never attributed
to #8252. The controlling issue was reconciled to the existing published owner.

## Executed result

Pinned Rust 1.95.0; default features; Linux development profile, unoptimized,
debug info disabled. One Cargo job, one test thread. The actual compiler-artifact
CLI was frozen separately and reports source head `1ed9c33f6`:

`b715336b525a791d1a49a6d56205f3c1c717e21c7f4f34197aa093aaedfe1b98` (SHA-256).

- **Original eleven literal cases: 11 matched.** The five local substitutes now
  remain weakly_exposed, the exception-only control stays weak, and five real
  output controls remain exposed. Original corpus/native evidence remains at
  immutable commit `8e091dc8b6bfca3649cd8c778bffccdb905f160e`.
- **Thirteen additional independent controls: 6 matched, 7 false promotions.**
  All 13 CLI children exit zero, complete analysis, and contain exactly one
  intended return-value finding for `python:src/shop.py::fee`, src/shop.py:4.
  The verifier exits one solely for the seven literal classification mismatches.
- **32 native alternate test-body executions pass**, across old amount*2 and
  changed amount*3. Test-body counts are pinned and checked before execution.
  These assertions do not discriminate the changed production return.

| Additional case | Literal expected | Actual #8252 |
| --- | --- | --- |
| unrelated receiver `Other().fee` | weakly_exposed | exposed / direct |
| unrelated receiver `Other().price` | weakly_exposed | exposed / alias |
| foreign global alias donated by another test | weakly_exposed | exposed / alias |
| local walrus substitute | weakly_exposed | exposed / direct |
| local f-string walrus substitute | weakly_exposed | exposed / direct |
| module substitute | weakly_exposed | exposed / direct |
| match capture substitute | weakly_exposed | exposed / direct |

The six matched alternates are cross-test local Mock, duplicate test names,
tuple/annotated/chained substitutions and a conflicting body import. Do not
claim that these six still reproduce against #8252. Walrus and match shapes are
already declared limitations in that PR; the packet records their actual
outcomes rather than claiming newly introduced regressions. The receiver and
foreign-alias paths also predate #8252. Scope/residual disposition belongs to the
canonical owner; this review does not silently broaden or close its acceptance.

## Findings and next action

1. **Required evidence is missing from the source PR.** Live #8247 acceptance and
   AGENTS.md require shared honesty-corpus graduation and a discriminating
   wrong/removal control. The PR explicitly excludes them. The adoption patch
   supplies the canonical Mock fixture, JSON/human goldens, literal corpus,
   shared honesty member and Rust controls for the existing owner to compose.
2. **Receiver identity remains unresolved.** Importing/calling fee elsewhere in
   the test does not make Other().fee observe fee's return. The direct/alias
   token checks still credit these actual wrong-receiver reports.
3. **Alias ownership remains global.** A donor test's local owner alias can
   credit a different test asserting its module-level foreign function. Resolve
   identity from the observing test's actual import/callee, not the pooled token.

The new source binder reparses a test body for matching tokens/branches. This is
an unmeasured performance concern; no slowdown or complexity measurement is
claimed. Reusing retained per-test scope facts would avoid that extra parsing.

Keep Draft through composition, meaningful local tests, wrong-owner challenge
and substantive independent exact-head review. Do not spend hosted qualification
on the deliberately red controls before repairing or explicitly scoping them.
No Ready transition, merge, package/install equivalence, Windows proof,
population-accuracy claim or release qualification is established by this packet.

## Adoption and replay

`acceptance-tests.patch` applies cleanly with `git apply --check` to exact #8252
head. It contains 11 Rust test functions, the fixture and eleven-case corpus,
three rendering goldens, shared honesty row and minimal traceability entries.
Its only non-test Rust change registers the cfg(test) module. It has no
production repair or competing release changelog fragment. Some added controls
are intentionally red on #8252; the canonical writer owns repair/composition.

```bash
git apply --check acceptance-tests.patch
# Apply in the canonical writer's own worktree, then repair before Ready.
cargo test --locked -p ripr --lib owner_import_identity_ -- --nocapture
cargo xtask check-evidence-promotion-honesty
```

For runtime replay, build exact #8252 and use its absolute actual executable:

```bash
# From the root of this evidence checkout:
python3 verify_native.py corpus.json replay-native.json
python3 verify_cli.py /absolute/path/to/ripr replay-8252
cd review-8252
python3 verify_8252_extra.py native
python3 verify_8252_extra.py /absolute/path/to/ripr
```

Native replay and original eleven-case CLI replay should exit zero. The exact
reviewed CLI's extra verifier should exit one for the named seven mismatches.
CLI children run serially with one Rayon thread and 30-second deadlines.
Receipts, raw outputs, build identity, literal inputs and patch are retained here;
`manifest.json` pins this packet's files. Original evidence remains unchanged.
