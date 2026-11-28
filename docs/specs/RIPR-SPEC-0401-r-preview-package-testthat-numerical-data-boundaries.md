# RIPR-SPEC-0401: R Preview Package, testthat, Numerical, and Data Boundaries

Status: proposed

Owner: language-adapter / analysis

Created: 2026-11-02

Linked proposal:

- [RIPR-PROP-0001: Multi-Language Adapter Preview](../proposals/RIPR-PROP-0001-multi-language-adapter-preview.md)

Linked ADRs:

- None yet (parser/fact-substrate ADR is a sibling leaf under #5592)

Linked plan:

- Epic [#5592](https://github.com/EffortlessMetrics/ripr-swarm/issues/5592) (R preview evidence programme under #5582)

Linked issues:

- [#5850](https://github.com/EffortlessMetrics/ripr-swarm/issues/5850) (this contract)
- [#5592](https://github.com/EffortlessMetrics/ripr-swarm/issues/5592) (parent epic)
- Shared architecture consumed by reference: #5526, #5531, #5532, #5541, [#5542](https://github.com/EffortlessMetrics/ripr-swarm/issues/5542), #5546, [#5559](https://github.com/EffortlessMetrics/ripr-swarm/issues/5559), #5560

Linked PRs:

- None yet

Support-tier impact:

- No tier change. R remains absent from default enablement and from gate, badge,
  baseline, RIPR Zero, and stable support claims until separate promotion
  contracts land. This spec defines preview/advisory boundaries only. The
  canonical support boundary remains
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml`, `docs/spec-index.d/`,
  `.ripr/traceability.toml`, and `metrics/capabilities.toml` (planned
  capability row only).
- No new crate feature, default `[languages]` entry, parser dependency, R
  runtime, package install, test execution, or native build is introduced by
  this document.

## Problem

R repositories combine package structure (`DESCRIPTION`, `NAMESPACE`, `R/`),
script and vignette surfaces, S3/S4/R6 dispatch, non-standard and tidy
evaluation, tolerance-based numerical assertions, rich data-frame semantics,
testthat editions, snapshots, and native `.Call`/`.C`/Rcpp edges. A
call-and-branch-centric evidence model cannot safely merge those facts without
explicit, typed boundaries.

Implementation must not begin without a single accepted contract that separates
package/script roles, framework registration, method-system owners, numerical
versus data versus condition versus snapshot versus side-effect oracles, and
cross-language native edges. Without that contract, an R adapter would either
stay unusably conservative or over-credit symbol names, package dependencies,
snapshot files, and tolerance wording.

This spec is the first leaf under #5592. It defines static-fact and oracle
**meaning** only. Parser routing, fact substrate, owner extraction, and
testthat registration are later siblings; they must conform to this contract.

## Behavior

### Relationship to shared architecture

The language-neutral adapter boundary, router posture, additive output metadata,
and preview non-goals remain
[RIPR-SPEC-0026: Language adapter contract](RIPR-SPEC-0026-language-adapter-contract.md).

Repo-mode disposition, analysis scope evidence lines, and capability cell lookup
follow #5531 and
[RIPR-SPEC-0365: Capability manifest cell join](RIPR-SPEC-0365-capability-manifest-cell-join.md).

Test **registration** is not test **activation**, and activation is not oracle
**eligibility**. R testthat facts must project into the shared activation and
oracle-eligibility model defined under #5542 (DTO names and vocabulary consumed
by reference; this spec does not duplicate that issue's closed vocabulary).

Native and other cross-language reach must use registered provenance under
#5559 and [RIPR-SPEC-0378: Cross-language edge registry v1](RIPR-SPEC-0378-cross-language-edge-registry-v1.md).
Symbol names, file extensions, or `DESCRIPTION` Depends fields must not mint
positive cross-language edges.

Verification orchestration for later calibration consumes #5162 by reference; no
R-only planner is defined here.

### Wire identity and file routing

- Wire language identity: `r` (JSON field `language = "r"`).
- Canonical routed extensions: `.R` and `.r` (case preserved in paths; routing
  treats both as R source).
- `language_status`: `preview` for every R finding until a separate promotion
  contract changes it.
- R is opt-in only through `[languages] enabled` (and future explicit R
  detection rules in a routing sibling). This spec does not default-enable R.

When R is disabled or the adapter is not compiled, changed `.R`/`.r` files may
appear in diffs without R preview analysis; that absence is not evidence that
behavior is untested.

### Package, script, and project roles

The adapter distinguishes these **source roles** (independent capability
cells; discovery completeness may differ by mode):

| Role | Typical paths | Fact purpose |
| --- | --- | --- |
| `package_root` | `DESCRIPTION`, `NAMESPACE` | Package identity, exports/imports syntax (not runtime attach) |
| `package_r` | `R/*.R` | Exported/internal function and object owners |
| `package_testthat` | `tests/testthat/**` | testthat tests, helpers, snapshots |
| `script` | `*.R` at repo root or `inst/` scripts | Top-level/script owners |
| `vignette` | `vignettes/*` | Narrative code; weak test evidence by default |
| `helper` | `tests/testthat/helper-*.R`, `R/` helpers | Setup/teardown and shared test utilities |
| `generated` | `R/*.R` marked generated or under known codegen paths | Facts tagged `generated`; no silent promotion to hand-authored |
| `vendor` | `inst/extdata`, vendored sources | Static inventory only; no install/compile |
| `native_source` | `src/*.c`, `src/*.cpp`, `src/*.f` | Native edge **declarations** only; execution crosses #5559 |

Parsing `DESCRIPTION` for Version/Maintainer is optional metadata only. Depends,
Imports, Suggests, and LinkingTo entries do **not** establish reach, observation,
or cross-language proof.

### Capability cells (registry identities)

Until R cells are registered in `domain::language_capability::CAPABILITY_CELLS`,
each combination below is **`not_proven`** in `ripr capabilities --json`. Cell
ids use the closed axes from `cell_id()` — language, framework, mode (`diff` or
`repo` only), and family (`static_facts` or `repair_routing` only). Finer
package/script/oracle distinctions are **facets** (next subsection), not
separate registry ids unless a future registry extension adds axes under #5531.

| Cell id | Language | Framework | Mode | Family | Intended state at first implementation |
| --- | --- | --- | --- | --- | --- |
| `r/any/diff/static_facts` | `r` | `any` | `diff` | `static_facts` | `preview` / syntax-first diff facts |
| `r/any/repo/static_facts` | `r` | `any` | `repo` | `static_facts` | `preview` / bounded repo scan |
| `r/testthat/diff/static_facts` | `r` | `testthat` | `diff` | `static_facts` | `preview` / testthat registration and oracles |
| `r/testthat/repo/static_facts` | `r` | `testthat` | `repo` | `static_facts` | `preview` / repo-mode testthat inventory |
| `r/any/diff/repair_routing` | `r` | `any` | `diff` | `repair_routing` | `not_proven` until repair cards are specified |

### Capability facets (independent within static facts)

Issue #5850 requires explicit, independent **facets** inside the static-facts
cells above. Facets are recorded on findings and capability evidence lines; they
do not mint alternate `cell_id()` values.

| Facet | Scope | Independent from |
| --- | --- | --- |
| `package_script` | DESCRIPTION/NAMESPACE/R layout vs script owners | `testthat` framework facet |
| `testthat_edition` | edition 2 vs 3 markers | numerical/data oracle facets |
| `numerical_oracle` | tolerance + observed quantity | data-frame facets |
| `data_oracle` | shape, value, order, class, attribute, name, missingness | numerical facet |
| `condition_oracle` | errors, warnings, messages | snapshot facet |
| `snapshot_oracle` | snapshot files and helpers | side-effect facet |
| `side_effect_oracle` | state, files, mocks, options | return-value/data facets |
| `relation` | owner↔test reach | oracle strength |
| `repo_mode` | diff vs repo disposition (#5531) | diff-only facets |
| `actionability` | repair readiness | oracle registration |
| `native_edge` | #5559 provenance | ordinary R reach |
| `verification` | external proof hooks (#5162) | static oracle credit |

Package/script, testthat edition, numerical oracle, data oracle, relation,
repo mode, actionability, native edge, and verification facets remain
**independent**. A strong numerical facet does not repair a missing package
discovery facet.

### Owner facts and stable probe identity

Owners the R preview contract must be able to represent (statically, when
syntax permits):

- package functions and internal functions (`function`, closures)
- active bindings where assignment shape is syntactically apparent
- S3 generics and methods (generic name + class string when literal)
- S4 classes and methods (class/signature literals when apparent)
- R6 classes, public/private methods, and active bindings (declaration shape)
- script/top-level owners and `{` blocks executed at source level

Stable `probe.owner` identifiers are language-qualified and path-qualified:

```text
r:<normalized/path.R>::<qualified_owner>
```

Examples:

```text
r:R/discount.R::apply_discount
r:R/_cart.R::Cart$total
r:tests/testthat/test-discount.R::<test_that_block>
r:scripts/run.R::<top_level>
```

Assignment variants (`<-`, `=`, `->`, `->>`) must not fork probe identity when
they bind the same syntactic owner. Method re-export through `NAMESPACE` must
not credit a test that only mentions an import alias without observing the
changed sink.

A function name, test name, snapshot path, or `DESCRIPTION` dependency string
**cannot** establish reach or observation by itself.

### testthat framework facts

Recognized registration shapes (syntax-first; edition-specific rules are
producer-owned):

- `test_that("...", { ... })` and `it("...", { ... })` inside `describe()`
- `describe("...", { ... })` grouping (index only; no runtime execution claim)
- setup/teardown: `setup`, `teardown`, `setup_edition`, helper sourcing from
  `helper-*.R` and `tests/testthat.R`
- edition markers (`testthat_edition(3)`, `local_edition()`) recorded as facts,
  not evaluated
- skips and guards: `skip()`, `skip_if()`, `skip_on_cran()`, platform skips,
  `testthat::skip_if_not_installed()` — map to activation/eligibility via #5542
- snapshots: `expect_snapshot()`, `expect_snapshot_file()`, variant labels —
  record **snapshot oracle kind** separately from changed-output reach
- generated/parameterized cases: `test_that(!!!params)`, map/`lapply` registration,
  `parameterized`/`generated` tags on tests when syntax is explicit; otherwise
  `conditional_activation_unknown` or `framework_registration_only`

testthat **edition** (2 vs 3) is a distinct fact from test **activation** and
from oracle **strength**. Edition affects quasibook/ snapshot helpers only when
the producer documents the mapping; static analysis does not execute helpers.

Verify-command shape (when activation is `active` and test identity is resolved):

```text
Rscript -e 'testthat::test_file("tests/testthat/test-foo.R")'
```

or the repo's documented `devtools::test()` / `rcmdcheck` wrapper when the
project marker and config sibling spec pin it. Unresolved test file paths omit
verify commands while retaining related-test pointers.

### Oracle families (must stay distinct)

Each family below produces separate internal oracle records and evidence lines.
Merging families into a single "object exists" check is forbidden.

#### Numerical oracles

- **Exact equality**: `expect_identical(x, y)` and other comparators whose
  testthat semantics are exact by definition — records compared **expressions**
  and literal expected values when syntactically visible.
- **Approximate equality (default for `expect_equal`)**: `expect_equal(x, y)`
  with or without an explicit `tolerance=` argument. testthat applies a default
  tolerance when the argument is omitted; static analysis must treat omitted
  tolerance as **approximate**, recording `tolerance = default` (or the literal
  when visible) and the **observed quantity** (which subexpression is compared).
  An approximate oracle must not read as exact `exposed` merely because the
  assertion omits `tolerance=`.
- **Other numeric comparisons**: `expect_lt`, `expect_gt`, `expect_true`/`expect_false`
  on numeric comparisons — must record observed quantity and discriminator class
  (approximate numeric or broad smoke), not generic strong equality.
- **Forbidden promotion**: tolerance wording must not read as exact `exposed`
  without an exact comparator; `expect_true(is.numeric(x))` remains broad smoke.

#### Data-frame, vector, and table oracles

Separate facts (each may appear in one assertion):

| Fact kind | Examples | Notes |
| --- | --- | --- |
| `data_shape` | `expect_equal(nrow(df), 3)`, `expect_length(x, 5)` | Row/column/length shape |
| `data_values` | `expect_equal(df$col, c(1,2))`, `expect_equal(unlist(x), ...)` | Values without conflating order |
| `data_order` | `expect_equal(order(df$x), ...)` | Order as its own discriminator |
| `data_class` | `expect_s3_class(x, "tbl_df")`, `expect_type(x, "list")` | Class/type distinct from value |
| `data_attributes` | `attr(x, "labels")`, dimnames | Attribute literals when visible |
| `data_names` | `expect_named(x, c("a","b"))` | Names distinct from values |
| `data_missingness` | `expect_true(anyNA(x))`, `expect_equal(sum(is.na(x)), 1)` | Missingness pattern |
| `statistical_property` | `expect_gt(mean(x), 0)`, variance checks | Bounded statistical **claims** only; no validity proof |

Whole-object `expect_equal(df1, df2)` may cite multiple fact kinds but classification
must still expose which dimensions the assertion could discriminate (shape,
values, order, etc.) for the changed sink.

#### Condition oracles

- **Errors**: `expect_error(...)`, `expect_warning(..., regexp=)`, `expect_message`
- **Classes and conditions**: `expect_condition()`, `expect_snapshot(error = TRUE)`
- Condition text patterns are **message observers**, not proof the changed code
  path executed unless the assertion syntactically ties to the owner call

#### Snapshot oracles

- Snapshot files under `tests/testthat/_snapshots/` and inline snapshots are
  **output observers** of whatever was captured when the snapshot was created.
- Snapshot presence or `expect_snapshot()` call **does not** imply the current
  diff's changed behavior reaches the snapshotted expression.
- Snapshot mismatch helpers (`waldo`, `testthat` snapshot engines) are static
  pattern recognition only.

#### Side-effect and state oracles

Distinct from return-value and data facts:

- `withr::local_*`, options, working directory, env vars
- `mockr`, `with_mocked_bindings`, `testthat::local_mocked_bindings`
- file existence (`expect_file_exists`, `expect_equal(readLines(...), ...)`)
- random seed declarations (`withr::local_seed`, `set.seed`) — recorded; seed
  state is `environment_or_state_unknown` unless literals pin reproducibility
- external data files (`testthat::test_path`, `system.file`) — path strings
  do not prove IO occurred

### Relations, reach, and limitations

Related tests may point at owners through:

- direct calls and namespace-qualified calls (`pkg::fun()`)
- formula/lazy evaluation surfaces only when the producer accepts #5542
  eligibility for the specific assertion
- S3 dispatch: literal class strings in `expect_s3_class` do not prove dispatch
  to the changed method without call-site evidence
- S4/R6: static dispatch when generic/class literals appear at the call site;
  otherwise `dynamic_dispatch` or `metaprogramming`
- `source()`, `sys.source()`, `eval(parse(...))`, `get()`, `assign()` in tests
  or owners: `dynamic_source_or_environment`
- tidy evaluation, quosures, formulas: `nse_or_quosure_unresolved`
- active bindings and locked environments: `environment_mutation_unknown`
- native `.Call`, `.C`, `.Fortran`, `.External`, Rcpp exports: only through
  registered cross-language edges (#5559 / RIPR-SPEC-0378); otherwise
  `cross_language_oracle_visibility_unresolved` or `native_edge_unregistered`

Custom **expectation** or **reporter** functions are `opaque_custom_assertion_helper`
unless the producer proves an equivalent closed oracle mapping.

### Output and witness reuse

R findings use the same additive JSON/human/SARIF/LSP/agent fields as other
preview languages (`language`, `language_status`, `probe.owner`, `owner_kind`,
`static_limit_kind`, confidence basis from RIPR-SPEC-0026). No R-specific
report fork, gap-id namespace, or schema version bump is permitted by this spec.

Witness and classification layers consume:

- shared exposure classes (`exposed`, `weakly_exposed`, `reachable_unrevealed`,
  `no_static_path`, `infection_unknown`, `propagation_unknown`, `static_unknown`)
- shared repair readiness validator (fail-closed; no R-specific bypass)
- #5542 activation/eligibility DTOs when projecting testthat facts

Static vocabulary must not use runtime mutation terms (`killed`, `survived`,
`untested`, `proven`, `adequate`). Statistical validity is never claimed.

### Statistical and numerical non-claims

Approximate equality records tolerance and observed quantity; it does **not**
claim:

- numerical correctness of the underlying algorithm
- statistical validity of a test or model
- equivalence of floating-point semantics across platforms
- that a tolerance band is appropriate for the domain

Property-based or Monte-Carlo style tests (`testthat` wrappers around repeated
sampling) map to `property_based_test` or `statistical_property` limits unless
a closed literal seed and finite iteration table is visible.

## Required Evidence

This document is satisfied when the following **registration proof** passes on
the merging commit (no R adapter code required):

```bash
cargo xtask check-spec-format
cargo xtask check-spec-numbering
cargo xtask check-doc-artifacts
cargo xtask check-doc-index
cargo xtask check-static-language
cargo xtask check-traceability
cargo xtask check-capabilities
cargo xtask precommit
```

Future implementation slices add fixture corpora, verdict rows, and domain
cells; those are tracked under #5592 siblings, not under this spec's initial
acceptance.

## Non-Goals

- No R interpreter, `R CMD INSTALL`, testthat execution, covr, or native
  compilation during passive analysis
- No claim that syntax proves numerical correctness or statistical validity
- No universal NSE/tidy-eval resolution
- No generated tests, edits, or default `[languages]` enablement
- No support-tier promotion, CRAN publication, or release action
- No parser, adapter, routing, or Rust implementation in the #5850 slice
- No duplicate implementation of #5542, #5531, #5559, or #5560 semantics

## Acceptance Examples

1. **Approximate numeric with tolerance**

   Changed owner returns `sqrt(x)`; test contains
   `expect_equal(sqrt(2), expected, tolerance = 0.01)`.

   Expected contract behavior:

   - numerical oracle facet: approximate with recorded `tolerance = 0.01`
   - observed quantity: `sqrt(2)` compared to `expected`, not a generic numeric flag
   - classification cannot read as exact equality merely because `expect_equal`
     appears

1b. **Default-tolerance `expect_equal`**

   Test contains `expect_equal(actual, expected)` with no `tolerance=` argument.

   Expected: approximate numeric facet with `tolerance = default`; not exact equality.

2. **Data shape vs value**

   Changed owner alters column count; test has `expect_equal(nrow(out), 2)` and
   separate `expect_equal(out$val, 1:2)`.

   Expected: shape and value facts remain separate; neither subsumes the other.

3. **Snapshot without reach**

   Diff changes `R/foo.R`; test adds `expect_snapshot(list(result = foo(1)))`.

   Expected: snapshot oracle recorded; reach to changed sink requires call-site
   evidence — snapshot file path alone does not credit `exposed`.

4. **Skip remains related, not eligible**

   `skip_if_not_installed("curl")` wraps `expect_equal(foo(), 1)`.

   Expected: related pointer with activation `conditional_activation_unknown` or
   inactive per #5542; no ordinary oracle credit.

5. **Native edge**

   Owner calls `.Call("bar", ...)`.

   Expected: without a registered #5559 edge, classification stays limited with
   `cross_language_oracle_visibility_unresolved` or `native_edge_unregistered`;
   symbol name `bar` alone proves nothing.

6. **Package dependency mention**

   Test loads `library(dplyr)` but asserts on a local `compute()` owner.

   Expected: `library()`/Depends mention does not relate test to unrelated owners.

## Test Mapping

Planned under #5592 verification leaves:

- `fixtures/r-verdict-corpus/` (authored rows per oracle family and limitation)
- `cargo xtask verdict-corpus --language r` (future)
- spec-scope entries under `fixtures/r-verdict-corpus/spec-scopes/RIPR-SPEC-0401.toml`

Until those land, this spec's proof is the registration gates listed under
Required Evidence.

## Implementation Mapping

Planned sibling issues under #5592 (not this PR):

- routing/config/package discovery
- parser/fact substrate ADR and implementation
- owner/probe extraction across S3/S4/R6
- testthat registration and edition facts
- numerical/data/condition/snapshot classifier implementation
- shared #5542 projection for testthat producers
- bounded repo mode and output parity

No production module is authoritative for this contract until a later slice
 cites this spec ID in code and fixtures.

## Metrics

- `r_preview_contract_registered` — spec, traceability, capability matrix, and
  doc-artifact gates cite RIPR-SPEC-0401 (#5850)

Implementation slices add discriminating metrics (false-actionable rate,
oracle-family coverage); those belong to later #5592 children.
