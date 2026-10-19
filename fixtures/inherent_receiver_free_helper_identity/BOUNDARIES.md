# Executable owner identity boundaries

The 34 entries in `cases.json` bind complete native sources to expected repository
relations. `native_reaches` records the independent one-arm mutation result;
`relation` records bounded static admission. Neither rewrites the other.

| Hypothesis / controls | Native owner mutant | Repository full / compact | Diff |
| --- | --- | --- | --- |
| Module-local type shadow / genuine owner | Survives / fails | Refuse high / direct | Refuse high / direct |
| Dormant nested assertion or plain call, closure, async body / real outer call | Survives / fails | Refuse high / direct | Refuse high / direct |
| Invoked nested function | Fails | Conservative unsupported route | Conservative unsupported route |
| Explicit foreign helper / bound root helper (including turbofish) | Survives / fails | Refuse high / helper | Conservative affinity |
| Unique root helper plus a foreign callable value | Survives | Refuse high | Conservative affinity |
| Same-file test-module helper, bare and self-qualified | Fails | Helper | Conservative affinity |
| Wrong imported twin / imported root helper | Survives / fails | Refuse high / helper | Conservative affinity |
| Intermediate wrong import / actual root binding | Survives / fails | Refuse high / helper | Conservative affinity |
| Same-line sibling declarations, explicit foreign path | Survives | Refuse ambiguous identity | Conservative affinity |

Each full/compact case retains exactly one named test for each of the four owner
seams. The diff integration retains both changed-owner probes and every oracle row
of their named subject. Real outer calls preserve direct/high. Valid helper paths preserve
repository helper/high; that does not assert diff-helper support. The earlier
qualified/live receiver, same-name inherent wrapper, standard/sibling assertion
and discarded-assertion controls remain in the adjacent established suites.

The intermediate pair deliberately exercises the existing multiline bare-call
candidate route. Its test calls `bridge`; only the import/binding inside the
bridge changes. The initial qualified one-line bridge setup and failed positive
expectation remain in the PR evidence, with a pre-repair CLI showing that this
form was never an admitted candidate. Candidate absence is not evidence that the
new identity gate rejected an edge.

Nested function execution, arbitrary closure/async activation, factory/parameter/
field/cross-file value flow, general type/Cargo-target identity and complete macro
expansion are not added. Invoked nested bodies remain unresolved rather than
being credited by declaration. Existing runtime-truth, ideal/acceptable verdict
labels and false_actionable accounting remain independent of analyzer advice.
Performance concerns about repeated parsing and per-call `OwnerPinSyntax` cache
loss stay substantive and unbenchmarked under #8005.

Native replay: compile each complete source with
`rustc --edition=2024 --test --crate-name control <source> -o <separate-output>`;
run the original, replace only `Self::Object(value) => Some(*value),` with
`Self::Object(value) => None,`, then compile/run the mutant. Keep native sources
and executables outside CLI input roots. All originals must pass; the matrix
records which mutants fail. Raw argv, digests, exits and CLI JSON are retained in
the PR evidence packet. No existing fixture golden is re-blessed for this wave.

Same-file helper entry reuses lexical binding and call identity without requiring
the production owner pin's Cargo/cfg provenance. In particular a helper declared
inside `#[cfg(test)]` remains a real helper; module/file provenance alone cannot
refuse its genuine bare or self-qualified call. Unplaced/twin declarations and
operand-local items still withhold identity. The established non-root source-file
and mixed-helper controls independently challenge this distinction.

The extra two-subject `controls/same-line-callers.rs` control checks shared scope
custody: differently named callers on one line must not share a name-scope memo.
The `actual` test rejects the owner mutant; `decoy` survives it when run alone.
Both retain one row on each of four owner seams; only `actual` gets helper/high.
A same-name/same-line source lookup that cannot select one declaration refuses
instead of borrowing the first one. This is identity correctness, not a parsing
or `OwnerPinSyntax` performance claim.

Function type parameters can shadow module or nominal receiver-type names. The
three helper-scope controls refuse those declarations while preserving a genuine
module-qualified helper through an unrelated type parameter. This compares
names in the parser's declaration scope; no argument type, trait dispatch,
factory return or parameter/field value is followed. The two negative native
subjects survive the owner mutant and the genuine-module positive rejects it.

The twelve assertion-operand controls pair direct and free-helper calls inside
unpolled async/async-move bodies, dormant closures and nested item bodies with
actual outer calls and outer calls beside a discarded future. They use the same
executable-node boundary in the original function and the synthetic operand
parser. Native originals all pass; four genuine outer/mixed mutants fail while
eight dormant mutants survive. No existing golden is re-blessed.
