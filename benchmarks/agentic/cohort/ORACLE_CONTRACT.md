# Reference qualification and runtime quality (#12902, #12912)

The [core contract](CORE_CONTRACT.md) is the shared authority for native execution,
fresh evaluator-owned grading, fixed test artifacts and retained-quality checks.
Collection, reference selection and reporting reuse that owner. No static RIPR
classification is promoted to a runtime outcome.

## Frozen-task scoring

A catch requires a nonempty pristine pass, separately compiled mutant execution
with a test failure, then a restored-pristine pass on the exact same test subject.
Timeouts, compiler/build/process failures, zero or partial tests, changed inputs
and failed restoration are unresolved. Every selected mutant remains in the
intended frozen-task denominator, including malformed, duplicate and not-run rows.
Source and configuration come from the frozen task, not an agent's mutable HEAD.

These are small trusted Rust tasks using stock libtest and `cargo test --tests`.
Doctests, custom harnesses, hostile submitted code and arbitrary repository graphs
are outside the protocol. Execute opt-in agent tests only in a disposable,
secret-free environment; fresh workspace construction is not a security sandbox.
Internally consistent supplied receipts are not authenticated producer evidence.

## Reference selection is a different phase

`oracle.py` may call a model to author reference suites only when explicitly run.
Its selector consumes the same native evidence as frozen-task grading. A future
mutant set may include candidates with pass/fail/pass controls, exclude executed
survivors, and exclude compiler-rejected candidates only with actual compiler
error observations and valid restoration. Every candidate outcome is retained in
`<task>.qualification.json` alongside the selected list.

Timeouts, build failures, missing execution, invalid mutation data and broken
restoration make qualification incomplete rather than silently removing difficult
candidates. Incomplete qualification retains the existing selected list and emits
the unresolved qualification record. An empty selected set is invalid. Once frozen,
each arm must use the same task and selected mutations; arm-specific exclusions
would change the question being measured.

Model-authored reference suites can bias selection toward readily detected bugs.
The instrument does not remove that bias, establish general test adequacy or
validate old captures that lack the new native observations.

## Authored reference validation

`validate.py` runs weak and authored reference suites without a model, through the
shared scorer. It returns nonzero for incomplete evidence, empty populations,
unknown tasks or a selected task without a reference. With no task arguments it
selects only tasks containing authored references; optional real-code tasks do
not acquire references by inference. The printed structured output retains both
weak and reference outcomes. A successful validator is scoped to those exact
inputs, not a new agent cohort or a RIPR efficiency claim.

The shared selftest covers the real validator/selector/report paths with controlled
process-boundary inputs plus six native Cargo controls. Live reference generation,
provider compatibility and the effectiveness of a new cohort need their own
retained observations; test-suite success is not a substitute for them.
