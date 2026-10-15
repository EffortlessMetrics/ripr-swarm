# Independent review of PR #8004 at source53

Reviewed head: `53e22999d19d07a28a355b740d62c01c25aea541`.
Tree: `07100736ea448043831337f64d45c7a26b6d1b79`.
Parent: `43f00b54eeb239c240bb71553c30a7bde3a4e080`.

Disposition: **REPAIR_REQUIRED**. The three original reproductions are repaired,
but three additional native negative controls retain false high owner credit.
No Ready or merge clearance. No source edits, source branch push, bot messages,
thread resolution, Ready, merge, release, or credential changes were performed.

## Findings

### P1: Module-local type shadow bypasses repository direct-owner refusal

`test_grip_evidence/related_tests.rs:274` calls only
`inherent_receiver_call_resolves`, while the diff route uses
`owner_call_relation_reason` and its source/module type-shadow refusal. With an
owner at crate root and a different `tests::JsonValue::as_object`, the test calls
only the latter. Diff JSON correctly emits weak/low. Full repository JSON emits
direct/high and all four root-owner seams are strongly gripped. A fresh compact
summary also reports four strongly-gripped seams. Original and owner-mutant native
tests both pass; the paired genuine-owner positive kills the mutation.

Fix direction: route repository direct inherent admission through the complete
shared owner authority already used for helper last hops. Add the module-level
shadow as a full/compact/diff honesty control. This is an existing lexical shadow
refusal, not a request for general Rust type resolution.

### P1: Dormant nested function supplies executable-owner credit

`classify/related_tests.rs:3066-3072` traverses every descendant, including assertion
macros inside a nested function never called by the test. A test that only defines
`fn dormant() { let value = JsonValue::Object(7); assert_eq!(value.as_object(), Some(7)); }`
and asserts `7 == 7` receives direct/high diff and repository credit. All four
owner seams are strongly gripped in full and fresh compact output. Native original
and owner mutant both pass. Changing the test to actually invoke `dormant()` kills
the mutant, distinguishing declaration from execution.

Fix direction: do not descend into nested item/function bodies as test-executed
syntax. If nested helper execution is supported, admit it only through an actual
resolved call; otherwise withhold credit. Keep outer test statements and genuine
wrapper calls supported. Pin both native and production-output controls.

### P1: Explicit foreign free-helper path borrows an unrelated same-named helper

`test_grip_evidence/related_tests/context.rs:4020-4021` admits a free entry by name
and call spelling, without binding the observed path to the candidate helper.
A root `read_as_object()` genuinely calls the owner; sibling
`foreign::read_as_object()` only returns `Some(7)`. A test calling exclusively the
explicit foreign path receives helper/high root-owner credit and all four owner
seams become strongly gripped. Full and fresh compact outputs agree on the false
promotion. Original and owner mutant both pass; changing the test to call the root
helper kills the mutation. Diff retains only file affinity.

Fix direction: bind free-helper entry to its actual module/function identity, or
withhold ambiguous entry. Existing name graphs remain candidate evidence. Apply
the same rule at intermediate hops and preserve unique genuine free helpers.

All three findings reproduce on exact parent43 and are inherited/current, not
introduced by source53. Fresh baseline results are in `baseline43-results.json`.
The same baseline comparison independently shows the intended repair deltas:
wrapper file-affinity becomes helper/high, foreign-helper high is withheld, and
module-macro direct/high diff credit becomes weak/low. An inherited false promotion
remains a current source defect under the bounded evidence-promotion contract.

## Original repairs and positive controls

The original local wrapper retains repository helper/high. Declaration artifacts
do not supply the submitted declaration-control edge. Foreign/unknown inherent
entry, foreign last hop and helper module-type shadow controls withhold high
owner credit. The discarding module assertion macro withholds direct/high in diff
and full/compact repository consumers. Real standard and sibling-scope assertions,
qualified owner variants and a live outer receiver after an expired foreign shadow
retain direct credit. The corresponding positive native mutants fail; negative
mutants survive. There are 16 original/mutant pairs, all 32 compile and all 16
originals pass. Raw results preserve intentional mutant failures.

## Provenance and limits

All seven then-current review threads were inspected, including two new threads
on this head. The original foreign-helper, macro-shadow, live-outer and same-name
wrapper reports are repaired for their retained minimal controls. The performance
threads remain substantive: repeated parsing and the new per-invocation
OwnerPinSyntax discard run-scoped memoization. The source pattern is real; impact
is unbenchmarked and remains pending under the performance follow-up, not refuted
by semantic test passes. A changelog clarification is reasonable: retained wrapper
helper/high is a repository claim; diff intentionally remains weak/low. The raw
consumer outputs confirm that distinction. No thread was answered or resolved.

Independent default-feature checks on the clean exact head: 383 test-grip units,
175 related-test units, and 8 receiver integrations pass, zero failures/ignored.
These exercise the submitted controls while the new counterexamples demonstrate
their remaining coverage gaps. They do not establish a full workspace or hosted
qualification pass. Related all-state issue/PR searches found existing shadow
authority in #7036/#6951 and adjacent owner-pin #6948/#6905; those landed repairs
explain the existing refusal reused by diff, rather than superseding this current
repository-consumer gap. #7409 is identity-only helper-transfer work and explicitly
does not repair related-test/exposure promotion; no rival repair was created.

The author repair packet was fetched from `776209c1eab8512696f50fedcaf555028b6394b2`.
Its manifest digest is `1c7cedcbdee5760cd0c4e9de48d9bcd828506caa32593db6f76b88912b29262d`;
all 278 entries verify. Author-reported 453 goldens/four corpora/15 check-fast gates/
58-path precommit are retained author evidence, not independent reruns here.
The current repair changes no existing fixture golden or five retained verdict
observation rows. After the documented locked anyhow1.0.100 fetch, an independent
current-head CLI run emits exposed and findings/summary equal the existing golden.
No re-bless was performed. Five original observations remain false_actionable;
truth/ideal/acceptable labels are unchanged.

The executed author composition is `72e25151dd03a669575c57175321072e69b24d42`,
tree `c80d13b636ac00b1fc695c5f41ff42e058183556`, parents main7dee + source53.
The observed GitHub merge ref is `79e7404f09e6c9c602558c9a9508aca10d6b6772`,
tree `665505641bd21f018359268ed614a34b205bf45b`, parents main37a31f94 + source53.
It equals the author's previously unexecuted later-main tree, not the executed
composition. All three changed receiver/helper production files are byte-identical
between reviewed source53 and that remote merge; owner_pin.rs changes only
short_circuit module visibility. Later-main MCP sizing and deep unresolved-witness
naming do not repair or create the observed short direct/helper false promotions.
No local execution claim is made for the remote merge, and no hosted checkout
identity was inferred from the PR merge ref or run metadata.

At inspection, CI run38716233808 executes a Rust-only feature lane (job116359477125),
not required default-feature qualification. The connector lists first-page jobs/
runs only, so this is not an exhaustive all-checks receipt. Hosted default-feature,
full workspace and Windows qualification remain unestablished here. Ready can
request hosted qualification once source/integration review clears; these source
findings currently prevent that clearance. Performance issue #8005 remains separate
and unbenchmarked. #7415 stays open. General type/Cargo-target identity, arbitrary
factory/parameter/field/cross-file flow and complete macro expansion remain honest
unsupported limits; none is needed by the three minimal failures above.

The selected environment started successfully; there was no executor disconnect.
Initial wrong-cwd inspection/archive attempts and default Rustup-home setup failed
before proof; correct repository paths and the installed /workspace Cargo/Rustup
homes were then used. An exploratory CLI run initially had native source artifacts
inside its control root; those were moved out and all final CLI reports rerun on
clean input roots. Only final clean-root reports support these findings.

## Reproduction

Use Rust/Cargo1.95.0. Build the frozen head with default features and `--locked`.
Run `cargo test --locked -p ripr --lib analysis::test_grip_evidence::tests::`,
the analogous `analysis::classify::related_tests::tests::` selection, and
`cargo test --locked -p ripr --test private_inherent_receiver`.

Each `controls/<case>/` contains a standalone Cargo input, unified diff and raw
CLI outputs. Invoke the frozen absolute CLI with `check --root <case> --mode ready
--format json --diff <case>/diff.patch`; omit diff for `repo-exposure-json`.
Fresh `compact/<case>/` roots were independently analyzed with
`repo-exposure-summary-json`, which routes through compact classification.
Native sources live outside Cargo roots: compile each `native/<case>/{original,mutant}.rs`
with `rustc --edition=2024 --test --crate-name control ... -o <separate executable>`
and run it. Exact argv, compile/run exit codes and source digests are in
`native-results.json`. Do not place those native .rs artifacts inside CLI input
roots, since the analyzer discovers extra Rust files there.
