# RIPR-SPEC-0392: PHP Preview Evidence and Support Contract

Status: proposed

Owner: language-adapter / swarm

Created: 2026-11-02

Linked issues:

- [#5620](https://github.com/EffortlessMetrics/ripr-swarm/issues/5620)
- Parent epic [#5584](https://github.com/EffortlessMetrics/ripr-swarm/issues/5584)
- Shared architecture [#5526](https://github.com/EffortlessMetrics/ripr-swarm/issues/5526)

Consumes (normative shared law; no PHP schema fork):

- [#5531](https://github.com/EffortlessMetrics/ripr-swarm/issues/5531) language capability matrix posture
- [#5532](https://github.com/EffortlessMetrics/ripr-swarm/issues/5532) static witness vocabulary
- [#5541](https://github.com/EffortlessMetrics/ripr-swarm/issues/5541) seam and probe contracts
- [#5542](https://github.com/EffortlessMetrics/ripr-swarm/issues/5542) test activation and oracle eligibility
- [#5546](https://github.com/EffortlessMetrics/ripr-swarm/issues/5546) oracle strength model
- [#5559](https://github.com/EffortlessMetrics/ripr-swarm/issues/5559) cross-module and symbol relations
- [#5560](https://github.com/EffortlessMetrics/ripr-swarm/issues/5560) corpus evaluation framework

Linked specs:

- [RIPR-SPEC-0026: Language adapter contract](RIPR-SPEC-0026-language-adapter-contract.md)
- [RIPR-SPEC-0030: Preview evidence policy boundary](RIPR-SPEC-0030-preview-evidence-policy-boundary.md)
- [RIPR-SPEC-0062: Cross-language oracle graph](RIPR-SPEC-0062-cross-language-oracle-graph.md)

Support-tier impact:

- Initial PHP support is preview and advisory only. This spec grants no default
  feature, gate, badge, baseline, RIPR Zero, release, or registry authority.
  Canonical support posture remains [Support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register in `policy/doc-artifacts.d/RIPR-SPEC-0392.toml`.
- Register capability row `php_preview_evidence_contract` in
  `metrics/capabilities.toml` at `planned`.
- No parser, adapter, Composer/PHP execution, framework boot, or generated tests
  are introduced by this spec document alone.

## Problem

The PHP controller epic needs a normative preview contract before parser or
adapter code lands. Without a written contract, PHP work can drift into ad hoc
syntax matching, per-framework schema forks, silent promotion of preview
findings, or collapsed trait and data-provider provenance.

PHP combines object-oriented and functional idioms, Composer autoload rules,
PHPUnit and Pest test surfaces, magic methods, dynamic dispatch, DI containers,
and runtime includes. RIPR must define admissible static claims and explicit
refusals while staying on the shared language-neutral evidence spine defined by
RIPR-SPEC-0026.

## Behavior

### Authority boundary

PHP preview evidence uses the same additive output fields as other preview
languages (`language = "php"`, `language_status = "preview"`). It must obey
RIPR-SPEC-0030: visible and advisory by default, not gate-eligible, not badge
input, not RIPR Zero blocking debt, and not mutation-calibrated confidence
unless a later explicit promotion policy names PHP.

Strong assertion syntax does not imply activation, changed-observable
discrimination, or repair-packet readiness. Unknown reach is not a gap; it
routes to typed limits or conservative classifications.

### Identity and project context

| Field | Contract |
| --- | --- |
| Wire language | `php` |
| Cargo feature | `lang-php` (binary must be built with the feature before any PHP adapter runs) |
| Routed extensions | `.php` |
| Open/close tags | `<?php`, `<?=`, and mixed-template boundaries are parser-owned; non-PHP assets containing embedded PHP are out of scope unless the file is a routed `.php` source |
| Project markers | `composer.json` presence may enable auto-detection when `ripr.toml` is absent; explicit `ripr.toml` remains authoritative |
| Composer roles | Record `autoload`, `autoload-dev`, PSR-4, PSR-0, `classmap`, and `files` rules as static metadata only; unresolved autoload targets refuse positively rather than guess |
| Path roles | Classify paths under `vendor/` as third-party external: no exposure attribution or seam mutation on vendor-owned symbols |
| PHP version capability | Declare supported syntax bands (PHP 7.4 through 8.3+ at implementation time) as static capability metadata; do not execute PHP to discover version |
| Symbol identity | Namespaces, imports, aliases, files, and symbols remain repository-relative and portable; no host-absolute paths in IDs |

Repo configuration (future adapter slice) extends `[languages]`:

```toml
[languages]
enabled = ["rust", "php"]
```

When `php` is configured without `lang-php`, the shared availability resolver
must name the missing feature rather than silently skip PHP files.

### Language router

- `*.php` → PHP preview adapter when enabled and compiled.
- Non-PHP extensions pass through unchanged.
- PHP does not fork JSON schema versions; findings use the shared spine from
  RIPR-SPEC-0026.

### Owner facts

Owners the PHP adapter must recognise (stable identities, exact parser spans
required for every probe attachment):

| Owner kind | Scope |
| --- | --- |
| `namespace` | Namespace-level functions where stably owned |
| `class`, `interface`, `trait`, `enum` | Type declarations |
| `method` | Class/interface/trait methods |
| `function` | Global and namespaced functions |
| `closure`, `arrow_function` | Closures and `fn () =>` when stably owned |
| `property` | Properties and accessors when mutation probes attach |

Stable `probe.owner` shape:

```text
php:<normalized/path.php>::<qualified_owner>
```

Examples:

```text
php:src/Pricing.php::Pricing::applyDiscount
php:src/Concerns/Logs.php::Logs::info
php:tests/Unit/PricingTest.php::PricingTest::test_applies_discount
```

Trait provenance invariant: methods declared in a trait retain declaration
origin. A test exercising `Consumer::action` that comes from `use LogsTrait`
must record trait declaration provenance (`LogsTrait::action`) separately from
the consuming class. Provenance must not collapse into the consumer class alone.

### Probe facts (exact spans mandatory)

Every probe records byte/line-accurate parser spans on the changed sink. Probe
kinds include at minimum:

| Probe kind | Typical PHP surface |
| --- | --- |
| `predicate` | `if`, `elseif`, `switch`, `match` arms, ternary conditions |
| `return` | `return` expressions and generator `return` |
| `throws` | `throw` expressions and documented `@throws` only as syntactic context, not runtime proof |
| `state_mutation` | Property assignment, `$this->prop =`, array writes on owned state |
| `yield` | `yield` and `yield from` |
| `collection_content` | Array/list/map literal entries participating in changed behavior |
| `output` | `echo`, `print`, output buffers when syntactically explicit |
| `call_interaction` | Statically resolved call sites when callee identity is known |
| `boundary_value` | Compared literals and named boundary inputs attached to predicates |

When span alignment fails (partial parse, embedded HTML, ambiguous tag boundaries),
emit a typed static limit instead of inventing a probe.

### Test activation (PHPUnit and Pest)

Framework producers populate the shared activation and oracle-eligibility model
from [#5542](https://github.com/EffortlessMetrics/ripr-swarm/issues/5542). PHPUnit
and Pest share one evidence meaning; differences stay in producer rows, not a
PHP-specific public schema fork.

PHPUnit recognition (non-exhaustive; producer-owned syntax):

- Methods named `test*` or annotated `#[Test]` / `@test`
- `#[DataProvider('method')]` / `@dataProvider method` with statically
  resolvable provider methods and literal datasets
- Lifecycle: `setUp`, `tearDown`, `setUpBeforeClass` as context only
- Skip/incomplete/risky/depends/group annotations and attributes map to closed
  activation vocabulary (`inactive_skip`, `todo_or_pending`,
  `conditional_activation_unknown`, `filtered_or_not_selected`, etc.)

Pest recognition:

- `test('…', function () { … })`, `it('…', function () { … })`
- `describe` / `beforeEach` as registration context
- `->with([…])`, `->with('dataset')`, dataset closures; unresolved external
  datasets → `conditional_activation_unknown` or
  `parameterized_or_generated`, not default `active`

Distinct states that must not collapse:

- test declaration vs selected case vs active run vs skipped/incomplete/risky
- data-provider registration vs resolved dataset row vs unknown generation
- group-filtered or dependent tests that never execute in the static configuration

### Oracle facts

Oracle kinds map into the shared oracle strength model (RIPR-SPEC-0062 /
#5546). Illustrative PHPUnit/Pest surfaces:

| Strength | Examples |
| --- | --- |
| Strong exact | `assertSame`, `assertEquals` with exact expected, `expectException` with class, Pest `expect($x)->toBe($y)` |
| Strong observer | `expectOutputString`, snapshot hooks with explicit expected bytes (still preview; no runtime snapshot execution) |
| Weak | `assertTrue`, `assertNotNull`, `assertIsArray`, Pest `toBeTrue()` |
| Interaction | PHPUnit `expects($this->once())`, Mockery `shouldReceive` — mocking the observed target inverts the observation boundary |
| Callback/custom | Opaque custom constraints → `opaque_custom_assertion_helper` limit |

Mocking the method under test prevents positive reach even when expectations
look strong.

### Relations

Relation strength must distinguish, at minimum:

| Relation | Meaning |
| --- | --- |
| `direct_symbol` | Test text or signature directly references owner symbol |
| `inheritance` | Base class / interface method resolution |
| `trait_provenance` | Trait-declared method used on consumer |
| `data_provider` | Provider method → case row → test method |
| `dataset` | Pest dataset → closure test |
| `helper_fixture` | Shared helper or fixture file |
| `service_container` | Container-resolved type (usually limited) |
| `runtime_include` | `include`/`require` introduced symbol |
| `dynamic_call` | Variable class/function/method |

Related tests may remain pointers when activation or oracle eligibility blocks
credit; row-local blockers must not copy one file-wide limitation onto every
test row (#5542 / #5445 posture).

### Limitation vocabulary

Map or add typed limits (fail-closed; no silent `no_static_path` promotion):

| Limit kind | Trigger |
| --- | --- |
| `magic_method_dispatch` | `__call`, `__callStatic`, `__get`, `__set`, other magic routing |
| `dynamic_dispatch` | Variable functions `$fn()`, variable methods `$this->$m()`, `new $class` |
| `dynamic_property` | Variable property access |
| `reflection_runtime` | `ReflectionClass`, attributes interpreted only at runtime |
| `generated_proxy` | Generated proxies where source is not stable |
| `dependency_injection_container` | Service locators, container `get`, facades without static binding |
| `unresolved_composer_autoload` | Unresolved PSR/classmap/files autoload target |
| `runtime_include` | `include`, `require`, `require_once`, conditional definitions |
| `opaque_custom_assertion_helper` | User-defined assertion wrappers |
| `framework_bootstrap` | Kernel boot, global state mutation outside test body |
| `extension_native_boundary` | Extension or native calls without static model |
| `unsupported_syntax` | Parser gap |

Each limit emits explicit `static_limit_kind` (or successor shared vocabulary),
preserves related-test pointers when useful, and withholds repair-packet
readiness unless a separate promotion policy applies.

### Outputs

Findings use conservative RIPR classifications (`exposed`, `weakly_exposed`,
`reachable_unrevealed`, `no_static_path`, `infection_unknown`,
`propagation_unknown`, `static_unknown`). PHP preview findings carry preview
metadata and must not claim runtime mutation outcomes.

## Required Evidence

This document slice establishes the normative contract and acceptance examples
below. Implementation slices must add, before support promotion:

- a fixture corpus with at least one positive and one negative example per
  owner, activation, oracle, relation, and limit family named in this spec
- trait, inheritance, and data-provider provenance controls that fail if
  provenance collapses to the consumer class only
- PHPUnit and Pest cases proving shared activation/eligibility projection
  without PHP-specific JSON schema forks
- parser span invariants checked by corpus diff (exact start/end required)
- registration in traceability, capability matrix, and doc-artifacts (this PR)
- `cargo xtask check-spec-format` and `cargo xtask check-spec-numbering` pass

Until those fixtures exist, required evidence for *behavior* remains planned;
required evidence for *the contract document* is this spec plus index,
traceability, capability, and doc-artifact rows.

## Non-Goals

- No PHP parser or adapter implementation in this slice
- No Composer install, PHP execution, PHPUnit/Pest runners, or framework boot
- No generated tests, automatic edits, or provider calls
- No support-tier promotion, default feature enablement, or publication
- No runtime mutation, coverage, or CI gate authority claims

## Acceptance Examples

### 1. Owner + predicate + strong oracle (positive)

Production:

```php
<?php
namespace App;

final class Pricing
{
    public function applyDiscount(float $amount, float $threshold): float
    {
        if ($amount >= $threshold) {
            return $amount * 0.9;
        }
        return $amount;
    }
}
```

Test (PHPUnit):

```php
public function test_applies_discount_above_threshold(): void
{
    $pricing = new Pricing();
    $this->assertSame(90.0, $pricing->applyDiscount(100.0, 50.0));
}
```

Expected static evidence:

- owner `php:src/Pricing.php::Pricing::applyDiscount` with `method`
- predicate probe on `$amount >= $threshold` with exact span
- oracle: strong exact via `assertSame`
- missing discriminator example: `$amount === $threshold` boundary
- `language = "php"`, `language_status = "preview"`

### 2. Trait provenance (positive)

Trait:

```php
namespace App\Concerns;

trait Logs
{
    public function logInfo(string $message): void
    {
        $this->messages[] = $message;
    }
}
```

Consumer:

```php
namespace App;

use App\Concerns\Logs;

final class Consumer
{
    use Logs;
}
```

Test calling `$consumer->logInfo('x')` must relate to owner
`Logs::logInfo` with `trait_provenance`, not only `Consumer::logInfo` without
declaration origin.

### 3. Data provider registration (positive + unknown row)

```php
#[DataProvider('amounts')]
public function test_discount(float $amount, float $expected): void
{
    $this->assertSame($expected, (new Pricing())->applyDiscount($amount, 10.0));
}

public static function amounts(): iterable
{
    yield 'above' => [100.0, 90.0];
    yield 'below' => [5.0, 5.0];
}
```

Expected:

- provider method linked with `data_provider` relation to each case row
- statically resolvable literal rows may activate; unknown external generation
  → `parameterized_or_generated` or `conditional_activation_unknown`, not
  silent `active`

### 4. Pest dataset (positive)

```php
test('discount applies', function (float $amount, float $expected) {
    expect((new Pricing())->applyDiscount($amount, 10.0))->toBe($expected);
})->with([
    'above' => [100.0, 90.0],
]);
```

Expected: same shared eligibility meaning as PHPUnit; framework field differs in
producer metadata only.

### 5. Skipped test cannot donate `exposed` (negative)

```php
#[Group('slow')]
public function test_skipped(): void
{
    $this->markTestSkipped('skipped');
    $this->assertSame(1, (new Pricing())->applyDiscount(1.0, 0.0));
}
```

Expected: related pointer allowed; activation `inactive_skip`; no ordinary
oracle credit toward `exposed`.

### 6. Magic method dispatch (negative)

```php
public function __call(string $name, array $args): mixed
{
    return $this->container->get($name)(...$args);
}
```

Expected: `magic_method_dispatch` and/or `dependency_injection_container`;
fail-closed; no invented callee probe.

### 7. Dynamic dispatch (negative)

```php
$m = 'run';
$this->$m();
```

Expected: `dynamic_dispatch`; no positive reach through variable method name.

### 8. Mock inverts observation (negative)

```php
$mock = $this->createMock(Pricing::class);
$mock->expects($this->once())->method('applyDiscount')->willReturn(1.0);
```

Expected: interaction recorded; mocking observed target blocks positive
reach/discrimination on the real `Pricing::applyDiscount` sink.

### 9. Unresolved autoload (negative)

```php
use App\Unknown\Service;

new Service();
```

When Composer static analysis cannot resolve `App\Unknown\Service` to a project
file, emit `unresolved_composer_autoload` rather than attributing a seam.

### 10. Vendor boundary (negative)

Changed file under `vendor/` must not produce exposure attribution or repair
gaps on third-party symbols.

## Test Mapping

Planned tests:

- `fixtures/php_preview_trait_provenance`
- `fixtures/php_preview_data_provider_cases`
- `fixtures/php_preview_pest_dataset_activation`
- `fixtures/php_preview_skipped_test_no_exposed`
- `fixtures/php_preview_magic_method_limit`
- `fixtures/php_preview_dynamic_dispatch_limit`
- `fixtures/php_preview_mock_inverts_reach`
- `fixtures/php_preview_unresolved_autoload_limit`
- `fixtures/php_preview_vendor_boundary_refusal`

Future adapter unit tests under `crates/ripr/src/analysis/language/php/` implement
the corpus above. Until then, contract registration is validated by:

- `cargo xtask check-spec-format`
- `cargo xtask check-spec-numbering`
- `cargo xtask check-doc-artifacts`
- `cargo xtask check-traceability`
- `cargo xtask check-capabilities`

## Implementation Mapping

Planned modules (future slices; none required to merge this spec):

- `crates/ripr/src/analysis/language/php/` — parser-backed facts and probes
- `crates/ripr/src/config/` — `[languages]` enablement and detection
- Shared activation projection — consumes #5542 DTOs without PHP schema fork
- Output surfaces — reuse RIPR-SPEC-0026 additive fields

## Metrics

Planned PHP adapter metrics (names reserved; counters begin at implementation):

- `language_adapter_php_findings_preview`
- `language_adapter_php_owner_method`
- `language_adapter_php_owner_function`
- `language_adapter_php_owner_trait_provenance`
- `language_adapter_php_oracle_strong_exact`
- `language_adapter_php_oracle_weak`
- `language_adapter_php_static_limit_magic_method_dispatch`
- `language_adapter_php_static_limit_dynamic_dispatch`
- `language_adapter_php_static_limit_unresolved_composer_autoload`
- `language_adapter_php_test_activation_unresolved`

Capability matrix row: `php_preview_evidence_contract` (`planned`).
