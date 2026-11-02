# RIPR-SPEC-0391: Ruby Preview Static Facts

Status: proposed

Created: 2026-11-02

Linked issues:

- [#5606](https://github.com/EffortlessMetrics/ripr-swarm/issues/5606)
- Parent [#5583](https://github.com/EffortlessMetrics/ripr-swarm/issues/5583)

Linked authorities:

- [#5526](https://github.com/EffortlessMetrics/ripr-swarm/issues/5526) multi-language architecture
- [#5531](https://github.com/EffortlessMetrics/ripr-swarm/issues/5531) capability manifest
- [#5532](https://github.com/EffortlessMetrics/ripr-swarm/issues/5532) witness projection
- [#5541](https://github.com/EffortlessMetrics/ripr-swarm/issues/5541) seam and probe contracts
- [#5542](https://github.com/EffortlessMetrics/ripr-swarm/issues/5542) test activation law
- [#5546](https://github.com/EffortlessMetrics/ripr-swarm/issues/5546) oracle strength and completeness
- [#5560](https://github.com/EffortlessMetrics/ripr-swarm/issues/5560) corpus evaluation framework

## Problem

Ruby workspaces need a normative preview contract before parser selection,
adapter routing, or analyzer code lands. Ruby's metaprogramming, open classes,
dynamic dispatch, and dual test stacks (RSpec and Minitest) make silent
coercion to `no_static_path` or framework-specific output forks especially
dangerous.

This spec defines the per-language static facts a future Ruby preview adapter
must produce. The language-neutral boundary, router posture, additive output
metadata, and preview policy live in
[RIPR-SPEC-0026: Language adapter contract](RIPR-SPEC-0026-language-adapter-contract.md)
and [RIPR-SPEC-0030: Preview evidence policy boundary](RIPR-SPEC-0030-preview-evidence-policy-boundary.md).
Activation vocabulary maps through the shared test-activation contract (#5542);
oracle strength maps through the shared completeness contract (#5546). This
spec does not introduce Ruby-only evidence classes, gap schemas, or support
semantics.

## Behavior

Initial Ruby support is preview and advisory only. The spec grants no gate,
badge, baseline, RIPR Zero, default-feature, repair-loop, or publication
authority.

The future Ruby preview adapter is enabled by repo configuration:

```toml
[languages]
enabled = ["rust", "ruby"]
```

and the `lang-ruby` Cargo feature when compiled. It routes at least:

- `*.rb` source files;
- `Gemfile`, `*.gemspec`, `Rakefile`, and `config.ru` as project markers only;
- extensionless files whose first line is a shebang naming `ruby` or `env ruby`.

It emits the same RIPR fact families as other preview adapters and tags each
finding with `language = "ruby"` and `language_status = "preview"`.

The adapter is syntax-first. It must not depend on Bundler, Ruby, Rails,
RSpec, Minitest, or any runtime execution. When syntax-first analysis cannot
classify, the adapter emits an explicit `static_limit_kind` (or a Ruby-specific
limit token defined below) and conservative exposure classes such as
`infection_unknown` or `static_unknown`. It must not silently map unresolved
dynamic edges to `no_static_path`.

### Soundness invariant

Unknown or limited reach caused by Ruby dynamism is never a clean unreached gap.
Dynamic dispatch, metaprogramming, unresolved shared examples, opaque matchers,
and framework macros produce typed limits and unknown/limited classifications,
not `no_static_path` merely because a callee or activation path was unresolved.

### Identity and project scope

| Concern | Contract |
| --- | --- |
| Wire language | `ruby` |
| Feature gate | `lang-ruby` |
| Project identity | Repository-relative gem or application root from `Gemfile` / gemspec markers without executing Bundler |
| Source roles | `production`, `test`, `support`, `helper`, `generated`, `vendored`, `migration`, `task`, `config`, `fixture` |
| Vendor deny | Paths under `vendor/bundle`, installed gem stubs, and declared vendored trees are ineligible for mutation seams |
| Interpreter evidence | Optional Ruby implementation/version markers when declared in repo config; never inherit the host `ruby -v` silently |

Portable owner identities are repository-relative and language-qualified:

```text
ruby:<normalized/path.rb>::<owner_path>
```

Examples: `ruby:lib/pricing.rb::Pricing#apply_discount`,
`ruby:lib/pricing.rb::Pricing.apply_discount` (singleton),
`ruby:script.rb::<top_level>::run`.

Reopened classes and modules preserve declaration-file provenance. Two `class User`
blocks in different files remain two declaration sites; methods do not collapse
into a synthetic monolithic owner. Refinements and `prepend`/`include` edges
record mixin provenance without merging unrelated definitions.

### Owner facts

Owners the adapter must recognise:

- modules and classes (including reopenings);
- instance methods (`def name`);
- singleton/class methods (`def self.name`, `class << self`);
- top-level `def` in scripts;
- blocks, lambdas, and `Proc` literals only when bound to a stable assignment or
  named call-site identity the syntax exposes;
- refinements as scoped overlays without erasing the refined declaration site.

Owner kinds emitted in Ruby preview facts (additive to RIPR-SPEC-0026 where
needed):

- `module`, `class`, `instance_method`, `singleton_method`, `top_level_method`,
  `bound_block` (only when assignment identity is syntactically stable).

### Probe facts

Probes attach to exact source spans from the first implementation. Required probe
families include:

- predicate probes (`if`, `unless`, `case`/`when`, ternary, modifier forms,
  `&&`/`||` chains that guard behavior);
- return-value probes (`return`, implicit last expression of a method body);
- error-path probes (`raise`, `fail`, `rescue` shapes that change failure);
- yield/block-output probes (`yield`, `block.call`, observable block effects);
- state-mutation probes (instance variables, `attr_writer`, collection mutation);
- collection/content probes (array/hash/string mutation visible in syntax);
- output/logging probes (`puts`, `logger`, `Rails.logger`, stdlib log calls when
  syntactically obvious);
- call/interaction probes for statically resolved calls and mock expectations.

When classification fails, emit `static_limit_kind` instead of inventing probes.

### Ruby preview fact vocabulary

| Field | Values (bounded) |
| --- | --- |
| `language` | `ruby` |
| `language_status` | `preview` |
| `framework` | `rspec`, `minitest`, `unknown_test` |
| `owner_kind` | see Owner facts |
| `test_kind` | `rspec_example`, `rspec_context`, `minitest_case`, `minitest_method`, `shared_example`, `unknown_test` |
| `assertion_kind` | `exact_value`, `predicate_weak`, `exception_path`, `collection_content`, `output_log`, `state_change`, `mock_interaction`, `smoke_weak` |
| `probe_kind` | `predicate`, `return_value`, `error_path`, `yield_value`, `state_mutation`, `collection_content`, `output_log`, `call_side_effect`, `mock_interaction` |
| `static_limit_kind` | shared kinds from RIPR-SPEC-0026 plus Ruby-specific tokens below |
| `activation_state` | maps to shared #5542 vocabulary (active, focused, skipped, pending, filtered, generated, parameterized, unresolved_shared, unknown) |

Ruby-specific `static_limit_kind` tokens (stable enum decision for this spec):

- `ruby_dynamic_send` — `send`, `public_send`, `__send__`, `method_missing` paths;
- `ruby_metaprogramming` — `define_method`, `class_eval`, `module_eval`, `instance_eval`, `eval`;
- `ruby_open_class_rebind` — monkeypatch or replace an existing method body across files when the static owner chain is ambiguous;
- `ruby_dynamic_constant` — `const_get`, dynamic `autoload`, unresolved constant paths;
- `ruby_unresolved_mixin` — `include`/`prepend`/`extend` targets not resolved syntactically;
- `ruby_opaque_matcher` — custom RSpec matcher or Minitest assertion helper the adapter does not inspect;
- `ruby_unresolved_shared_example` — `it_behaves_like`, `include_examples`, or shared context without a resolvable definition;
- `ruby_framework_macro` — Rails/framework DSL macros and runtime DI hooks;
- `ruby_native_extension` — FFI/C extension or cross-language boundary;
- `ruby_custom_assertion_helper` — project helper that wraps expectations opaquely.

Reuse shared limit kinds when accurate (`dynamic_dispatch`, `metaprogramming`,
`missing_import_graph`, `unsupported_syntax`, `mocked_module`,
`decorator_indirection`).

### Test activation — RSpec

Map RSpec surfaces into shared activation states (#5542), not a parallel schema:

| Surface | Registration | Activation notes |
| --- | --- | --- |
| `describe`, `context`, `RSpec.describe` | example group | group filters/tags recorded syntactically only |
| `it`, `specify`, `example` | example | |
| Focus | `fit`, `fdescribe`, `fcontext`, `:focus => true` | `focused` |
| Skip/pending | `xit`, `xdescribe`, `:skip`, `:pending`, `pending` | `skipped` / `pending` |
| Shared examples | `it_behaves_like`, `include_examples` | `unresolved_shared` when definition not found |
| Hooks | `before`, `after`, `around` | hooks do not grant reach by presence alone |
| `let`, `subject`, factories | syntactic binding only | fixture-sourced discriminators stay limited |

### Test activation — Minitest

| Surface | Registration | Activation notes |
| --- | --- | --- |
| `class ... < Minitest::Test` (and `MiniTest::Test`) | test case | |
| `def test_*` | test method | |
| `setup` / `teardown` | lifecycle | no reach from hook presence alone |
| `skip`, pendings | | `skipped` / `pending` |
| Spec-style Minitest (`describe/it` when used) | map like RSpec when syntactically obvious | otherwise `unknown_test` |

RSpec and Minitest remain separate capability cells under #5531
(`ruby/rspec/*` vs `ruby/minitest/*`) even though output schemas stay shared.

### Oracles and relation strength

Bounded credit rules:

- **Strong**: `expect(x).to eq(y)`, `assert_equal exp, act`, exact `raise_error` /
  `assert_raises` with resolvable exception class, exact collection/hash keys,
  explicit log/output substring matchers with literal expected text, state reads
  on a local that is assigned once, passed into the changed method, and asserted
  after the call in the same example.
- **Weak**: truthiness (`be_truthy`, bare `assert obj`), type-only matchers,
  substring-free presence checks, title/proximity-only links.
- **Mock inversion**: `allow(...).to receive`, `expect(...).to receive`, Minitest
  stubs/mocks verify interaction contracts and refute execution of the underlying
  production implementation for credit purposes.

Relation strength ladder (strongest first, all syntax-bounded):

1. Direct call to resolved owner constant or `self`/explicit receiver;
2. Resolved constant/receiver chains (`MyApp::Service.new.call`);
3. Established helper chain in the same file with literal arguments;
4. Shared example or factory path only when statically resolved;
5. Rails-style macros and unresolved public API paths — limited/unknown.

A matcher token, example title, file proximity, or strong expectation elsewhere in
the same example cannot establish discrimination for an unrelated changed
observable (#5546 alignment rule).

### Capability and authority boundary

Under #5531, register separate framework/mode cells for at least:

- `ruby/rspec/diff/static_facts`, `ruby/rspec/repo/static_facts`;
- `ruby/minitest/diff/static_facts`, `ruby/minitest/repo/static_facts`.

Product status for all Ruby cells remains `planned` / `not_proven` until adapter
implementation and fixture proof land. No cell registration implies compile-time
availability of `lang-ruby`.

Diff mode and repo mode both disclose preview/advisory posture and name files
refused by budget or role filters explicitly.

## Required Evidence

This spec slice is documentation-only. Satisfaction requires:

- this document with positive and negative acceptance examples for every contract
  family above;
- `docs/spec-index.d/RIPR-SPEC-0391.md` index row;
- `.ripr/traceability.toml` behavior entry;
- `metrics/capabilities.toml` and `docs/CAPABILITY_MATRIX.md` rows for
  `ruby_preview_static_facts` in `planned` status;
- `cargo xtask check-spec-format`, `check-spec-numbering`, `check-doc-index`,
  `check-traceability`, and `check-capabilities` clean on the PR that lands the
  spec.

Future implementation PRs must add fixture-backed proof per #5560 before
promoting any Ruby cell beyond `planned`.

## Non-Goals

- No parser dependency selection (see #5607), adapter implementation, Ruby
  subprocess, Bundler/RSpec/Minitest execution, or generated tests.
- No support-tier promotion, release, publication, gate, badge, baseline, or
  RIPR Zero authority.
- No Ruby-only JSON schema fork or parallel activation/oracle enums in output.

## Acceptance Examples

Each example states expected static evidence vocabulary; none imply runtime
mutation outcomes.

### 1. Predicate boundary with strong oracle (positive)

```ruby
class Discount
  def apply(amount, threshold)
    amount >= threshold ? amount * 0.9 : amount
  end
end
```

```ruby
RSpec.describe Discount do
  it "applies above threshold" do
    expect(Discount.new.apply(100, 50)).to eq 90
  end

  it "does not apply below" do
    expect(Discount.new.apply(10, 50)).to eq 10
  end
end
```

Expected:

- owner `Discount#apply`, predicate probe on `amount >= threshold`;
- strong `exact_value` oracles on both examples;
- missing discriminator at `amount == threshold`;
- `language = "ruby"`, `language_status = "preview"`, `framework = "rspec"`.

### 2. Missing boundary discriminator (negative control)

Same production change as example 1 but only one-sided tests. Expected: gap or
weakly_exposed boundary family remains; adapter must not claim `exposed` without
an equality pin at the boundary.

### 3. Minitest exception path (positive)

```ruby
def apply(amount)
  raise ArgumentError, "negative" if amount.negative?
  amount
end
```

```ruby
class ApplyTest < Minitest::Test
  def test_rejects_negative
    assert_raises(ArgumentError) { apply(-1) }
  end
end
```

Expected: `error_path` probe, strong exception oracle, `framework = "minitest"`.

### 4. Mock interaction inverts production credit (negative)

```ruby
expect(service).to receive(:total).and_return(42)
service.total
```

Expected: `mock_interaction` oracle; production `total` implementation must not
read as executed/observed for exposure credit.

### 5. Dynamic send limit (negative — must not become `no_static_path`)

```ruby
obj.send(:calculate, 1)
```

Expected: `static_limit_kind = "ruby_dynamic_send"` (or shared
`dynamic_dispatch`), classification `static_unknown` / `infection_unknown`, not
`no_static_path`.

### 6. `method_missing` limit (negative)

```ruby
def method_missing(name, *args)
  calculator.public_send(name, *args)
end
```

Expected: typed metaprogramming/dynamic-send limit; no false clean gap.

### 7. Reopened class provenance (positive)

File `a/user.rb`:

```ruby
class User
  def first; end
end
```

File `b/user_extra.rb`:

```ruby
class User
  def last; end
end
```

Expected: two declaration provenance records for `User#first` and `User#last`;
no merged synthetic owner erasing file paths.

### 8. Unresolved shared example (negative)

```ruby
it_behaves_like "billing customer"
```

with no local shared example definition. Expected:
`activation_state = unresolved_shared`, no oracle credit until resolved.

### 9. Opaque custom matcher (negative)

```ruby
expect(result).to be_successful_contract
```

where `be_successful_contract` is a project matcher. Expected:
`ruby_opaque_matcher` limit unless helper body is on the closed inspect list.

### 10. Focused example registration (activation)

```ruby
fit "only this" do
  expect(1).to eq 1
end
```

Expected: example registered as `focused`; ordinary examples in the same group
remain distinct under #5542.

### 11. Skipped example (activation)

```ruby
xit "skipped" do
  expect(f).to eq 1
end
```

Expected: `skipped` activation; must not suppress unrelated active examples.

### 12. Rails macro limit (negative)

```ruby
class OrdersController < ApplicationController
  def show
    @order = Order.find(params[:id])
  end
end
```

Expected: framework macro / unresolved public API path stays limited unless a
future closed allow-list says otherwise; no fake strong oracle from route macro
text alone.

### 13. Native extension boundary (negative)

Call into a C extension method with no Ruby source body. Expected:
`ruby_native_extension` limit at the call site.

### 14. Weak truthiness oracle (negative)

```ruby
expect(x).to be_truthy
```

Expected: `predicate_weak`; cannot alone credit a numeric or equality seam.

### 15. Exact span requirement (positive)

Any emitted probe or owner fact must cite exact start/end byte or line/column
spans in adapter output metadata from the first implementation; documentation
examples name spans conceptually even before parser choice.

## Test Mapping

Planned proof (not in this docs slice):

- `crates/ripr/src/analysis/language/ruby/tests.rs` — owner, activation, oracle,
  and limitation families pinned per acceptance example;
- `fixtures/ruby-preview-static-facts/` — golden check JSON for RSpec and Minitest
  pairs;
- `cargo xtask check-fixture-contracts` and `cargo xtask goldens check` once
  adapter code exists.

## Implementation Mapping

Planned modules (not in this docs slice):

- `crates/ripr/src/analysis/language/ruby/` — syntax-first fact extraction;
- `crates/ripr/src/config/` — `[languages] enabled` and Ruby project markers;
- shared output renderers — consume language metadata only; no Ruby fork.

Parser/substrate selection is explicitly out of scope (#5607).

## Metrics

- `language_adapter_ruby_spec_registered` — spec, traceability, capability, and
  doc index rows present;
- future: `language_adapter_ruby_findings_preview`,
  `language_adapter_ruby_static_limit_ruby_dynamic_send`, and per-framework
  activation/oracle counters once fixtures exist.
