# RIPR-SPEC-0404: PostgreSQL/pgTAP dialect, schema, migration, transaction, and database-state evidence contracts

Status: proposed

Owner: language-adapter / ecosystem (#5602)

Created: 2026-11-02

Linked issues:

- [#5736](https://github.com/EffortlessMetrics/ripr-swarm/issues/5736) (this contract)
- Parent epic [#5602](https://github.com/EffortlessMetrics/ripr-swarm/issues/5602)
- Programme [#5582](https://github.com/EffortlessMetrics/ripr-swarm/issues/5582)
- Shared architecture [#5526](https://github.com/EffortlessMetrics/ripr-swarm/issues/5526)
- Consumed authorities: [#5531](https://github.com/EffortlessMetrics/ripr-swarm/issues/5531), [#5532](https://github.com/EffortlessMetrics/ripr-swarm/issues/5532), [#5541](https://github.com/EffortlessMetrics/ripr-swarm/issues/5541), [#5542](https://github.com/EffortlessMetrics/ripr-swarm/issues/5542), [#5546](https://github.com/EffortlessMetrics/ripr-swarm/issues/5546), [#5559](https://github.com/EffortlessMetrics/ripr-swarm/issues/5559), [#5560](https://github.com/EffortlessMetrics/ripr-swarm/issues/5560)

Linked specs:

- [RIPR-SPEC-0026](RIPR-SPEC-0026-language-adapter-contract.md) (language-neutral adapter boundary)
- [RIPR-SPEC-0030](RIPR-SPEC-0030-preview-evidence-policy-boundary.md) (preview posture)
- [RIPR-SPEC-0378](RIPR-SPEC-0378-cross-language-edge-registry-v1.md) (registered cross-language edges)

Support-tier impact:

- This spec does not enable SQL analysis, promote support tiers, arm default gates,
  badges, baselines, RIPR Zero blocking, or publication. All PostgreSQL, pgTAP,
  and admitted procedural-language evidence governed here remains preview/advisory
  per dialect, server-version range, project, migration framework, and mode until
  separate promotion work cites proof. The canonical support-tier boundary remains
  [support tiers](../status/SUPPORT_TIERS.md).
- Other SQL dialects, ORMs, and query builders do not inherit PostgreSQL support
  from this spec.

Policy impact:

- Register in `policy/doc-artifacts.d/RIPR-SPEC-0404.toml`.
- Add `docs/spec-index.d/RIPR-SPEC-0404.md`.
- Register `.ripr/traceability.d/5736-postgresql-pgtap-evidence-contracts.toml`.
- Register `fixtures/*/verdict-corpus/spec-scopes/RIPR-SPEC-0404.toml` as out of
  scope for runtime mutant corpora until SQL judged corpora land under #5602.
- No adapter, parser, routing, feature gate, database connection, or `LanguageId`
  registration is introduced by this document alone.

## Problem

SQL behavior is dialect-specific, schema-qualified, migration-ordered, and often
observed through pgTAP harnesses rather than ordinary unit-test returns. Passive
analysis could treat `.sql` as generic ANSI SQL, collapse migrations into a
timeless final schema, credit pgTAP function names without activation, or infer
live database state from repository files alone.

Epic #5602 needs one normative **project/dialect/migration substrate** law and
separate **PostgreSQL semantic**, **pgTAP test/oracle**, and **database-state
uncertainty** laws before ADR selection, routing, or adapters ship.

## Behavior

### Wire language and independent capability cells

The closed wire language is exactly:

```text
sql
```

Dialect, procedural language, migration framework, and test harness are
**independent capability cells**, not implied by file extension alone. A `.sql`
path MUST NOT select a dialect without project/configuration evidence.

| Cell dimension | Initial admitted values | Non-inheritance |
| --- | --- | --- |
| Dialect / server syntax | PostgreSQL (declared version range) | ANSI-generic, SQLite, MySQL/MariaDB, SQL Server/T-SQL, Oracle/PLSQL, BigQuery, Snowflake, Redshift, DuckDB |
| Test SQL | pgTAP (extension + admitted function set) | Ordinary `SELECT` assertions without pgTAP evidence |
| Procedural language | PL/pgSQL only when separately evidenced | Other PLs unless future spec slices admit them |
| Source roles | migration, schema, query, test, fixture, seed, generated, vendor | ORM/query-builder strings without #5559 edges |
| Project context | repository-selected root, schema lineage, migration track | Live server identity, connection string secrets |

PostgreSQL support does not imply any row in the non-inheritance column. dbt,
ORM-generated SQL, and application-embedded queries require their own wire
language or registered cross-language edges under
[RIPR-SPEC-0378](RIPR-SPEC-0378-cross-language-edge-registry-v1.md).

### Four machine-readable contracts

This spec defines four separable JSON-shaped contracts. Implementations may store
them in one envelope, but validation and promotion MUST keep the slices
independent:

| Contract | `schema_version` | Owns |
| --- | --- | --- |
| SQL project/dialect substrate | `sql_project_substrate.v1` | Dialect/version, project root, migration framework, schema/search-path declarations, source roles, extension declarations |
| PostgreSQL semantic evidence | `sql_language_evidence.v1` | Owners, probes, limitations for `dialect = postgresql` |
| pgTAP test/oracle evidence | `sql_test_oracle_evidence.v1` | Registration, activation, oracle families for pgTAP |
| Database-state disposition | `sql_database_state_disposition.v1` | Live catalog/runtime state unknowns, migration applied-state, external receipts |

Each document requires `validate()` after deserialization, uses
`deny_unknown_fields`, and records explicit `currentness` for facts that can go
stale (migration head, declared server version, extension version, selected
project root, schema dump producer).

Substrate facts MUST NOT appear inside semantic or oracle documents except by
reference (`substrate_ref` id + digest). Substrate fields alone MUST NOT claim
reach, exposure, or oracle strength.

#### SQL project/dialect substrate (`sql_project_substrate.v1`)

Portable identities without database execution:

- **Dialect and version range**: declared PostgreSQL major/minor or compatible
  range from configuration; otherwise `dialect: unknown` with typed limitation.
- **Project root**: repository-relative root with `selection_basis`
  (`explicit_config`, `single_tree`, `ambiguous`).
- **Migration framework**: Flyway, Sqitch, golang-migrate, dbmate, Rails
  `db/migrate`, custom — each with `framework_id`, config paths, and **ordered**
  migration identities (`migration_id`, `version`, `repo_path`, `source_span`).
- **Schema context**: declared default schema, `search_path` literals when
  statically present, temporary-schema usage flags, session role/settings literals
  in setup scripts (declarations only).
- **Source roles**: each `.sql` (or embedded region) tagged
  `migration`, `schema`, `query`, `test`, `fixture`, `seed`, `generated`, or
  `vendor`.
- **Extensions**: `CREATE EXTENSION` and extension-owned objects as external,
  versioned facts linked to declared versions — not repository-owned definitions
  unless source is present.
- **Schema snapshots/dumps**: declared artifacts with `producer`, `captured_at` or
  `unknown`, and digest — never automatically canonical live truth.

**Migration law:** A migration is an **ordered state transition**, not a
timeless final-schema declaration. Fields MUST distinguish baseline/head,
repeatable/undo/down migrations, branching/squashing/baselining, multiple
tracks, and already-applied external state (`applied_state:
unknown|declared|receipt_linked`). Changing an earlier migration versus adding a
new migration are distinct subject movements with separate probe identities.
Passive analysis MUST NOT “execute” migrations to reconstruct database state.

**Live-state law:** Current live database/catalog state is `unknown` unless a
separately governed catalog or runtime receipt is linked in
`sql_database_state_disposition.v1`. Repository SQL alone cannot establish row
contents, sequence values, or applied migration history on a server.

#### PostgreSQL semantic contract (`sql_language_evidence.v1`, `dialect = postgresql`)

Object and statement identities MUST include dialect, project/migration/schema
context, qualified name or signature, object kind, exact source span, and
authored/generated role. Silent normalization is forbidden for `search_path`,
temporary schemas, session role, overloaded signatures, quoted identifiers, case
folding, and unresolved qualification.

Portable owner identities include:

- **Logical project context** for databases (not live server identity).
- **Schemas**; **tables**, **views**, **materialized views**, **sequences**,
  **indexes**, **constraints**, **columns**, **types**, **domains**, **enums**,
  **collations**, **policies**, **publications/subscriptions** where admitted.
- **Functions/procedures**: schema-qualified name, argument modes/types, return
  type, language, volatility/security/parallel/strict/leakproof when declared.
- **Triggers**, trigger functions, **event triggers**, **rules**, generated/default
  expressions, **RLS policies**, **grants/roles** as declarations.
- **Migrations** as ordered operations with lineage/currentness, not as detached
  DDL blobs.
- **Queries/statements**: statements, CTEs, subqueries, expressions with exact
  spans; several expressions on one line receive distinct identities.
- **Extensions** and extension-owned objects per substrate linkage.

**Probe families** (exact-span mandatory):

| Family | Covers |
| --- | --- |
| Predicate / filter | WHERE, JOIN ON, CASE arms, HAVING and dialect-specific clauses only when admitted, bounds, null semantics, casts, collations, ordering, LIMIT/OFFSET, aggregates/windows, DISTINCT, grouping, set ops, row/cardinality behavior |
| Projection / result shape | Selected/projected values, result-set shape/order/type/nullability |
| DML / state change | INSERT/UPDATE/DELETE/MERGE-like writes, defaults/generated, constraints, cascades, triggers, policies, transactional state changes |
| Callable outcomes | Function/procedure returns, exceptions/SQLSTATE, notices/warnings when statically representable |
| DDL / migration ops | Object shape and migration operation semantics |
| Security context | SECURITY DEFINER/INVOKER, RLS policies, grants — declarations/limitations only, not runtime authorization proof |
| Dynamic / procedural | Dynamic SQL and PL/pgSQL bodies only at strength the accepted substrate proves |

Permission and RLS effects are declaration boundaries, not proof of runtime
authorization outcomes.

#### pgTAP test/oracle contract (`sql_test_oracle_evidence.v1`)

Maps exact pgTAP extension/function identities and admitted versions into
[#5542](https://github.com/EffortlessMetrics/ripr-swarm/issues/5542).

**Activation precedes oracle eligibility:** registration, planning, setup,
fixture/transaction/savepoint/role/search_path/config statements, assertion calls,
todo/skip/bailout/finish, extension availability, and selected test file/migration
context are distinct fields. Oracle credit is impossible when activation is
`inactive`, `skipped`, `todo`, `generated_unknown`, `dynamic_unknown`, or
`unknown`.

| Harness surface | Distinct from |
| --- | --- |
| `plan` / `no_plan` / `done_testing` | Object reach, oracle adequacy, production activation |
| Setup/fixture/transaction SQL | Individual assertion oracles |
| Assertion function call + description | Registration or planning functions |
| Known literal loops/cases | Procedural/dynamic/generated tests without static cases |

A pgTAP function name, test description, or extension declaration does not prove
active execution.

**Oracle families** (bounded pgTAP facts where statically established):

- Scalar equality/inequality/near/comparison/type/null/boolean checks.
- Result-set values, rows, columns, cardinality, order, types, nullability, names,
  tied to exact query/source identity.
- Schema/object/column/type/index/constraint/function/trigger/policy/privilege
  existence and shape assertions.
- Errors, SQLSTATE, message/detail/hint/context where supported.
- Function/procedure return and side-effect observations where exact.
- Transaction/rollback/setup postconditions where exact.
- Plan/explain/performance assertions only as separate capability — never ordinary
  correctness.
- Custom assertion functions only through exact source/body semantics.

**Oracle non-collapse law:** Object existence is not behavior discrimination. A
row-count assertion does not prove row values. Result-set order is not guaranteed
without established ordering. A strong schema assertion elsewhere cannot grip a
changed query or trigger.

#### Database-state disposition (`sql_database_state_disposition.v1`)

Records what repository analysis cannot know:

- `live_catalog_state`: default `unknown`; optional `receipt_ref` to governed
  catalog/runtime packet.
- `migration_applied_state` per track: `unknown` unless receipt or explicit
  declared metadata links applied versions.
- `session_snapshot`: role, `search_path`, GUC settings, temp schema — only when
  declared in source; otherwise typed limitations.
- `concurrency/isolation`: isolation level declarations vs runtime behavior —
  runtime remains unknown without receipt.

### Dependency, trigger, policy, and dynamic behavior

Map or emit typed stops (extend only via spec amendment):

| Limitation kind | When |
| --- | --- |
| `sql_search_path_unresolved` | Qualification depends on unset `search_path` or temp schema |
| `sql_overload_unresolved` | Function/operator overload set not fixed statically |
| `sql_implicit_cast_or_operator_unresolved` | Implicit cast, domain, collation, extension operator |
| `sql_dynamic_sql_unresolved` | Dynamic SQL, EXECUTE, format/quote identifiers, prepared params |
| `sql_procedural_body_unresolved` | PL/pgSQL control, handlers, cursors, polymorphism |
| `sql_trigger_or_rule_unresolved` | Triggers, event triggers, rules, cascades, deferred constraints |
| `sql_rls_or_policy_unresolved` | RLS, policies, security definer/invoker, privileges, role inheritance |
| `sql_sequence_or_identity_unknown` | Sequences, identity, time/random/locale/timezone/encoding |
| `sql_concurrency_or_isolation_unknown` | Locks, advisory locks, serialization, deadlocks |
| `sql_external_object_unresolved` | FDWs, listeners, background workers, external tables |
| `sql_migration_state_unknown` | Ordering, branching, squashing, applied external state |
| `sql_live_database_state_unknown` | Any claim requiring live rows/catalog not linked by receipt |
| `sql_cross_language_edge_missing` | Application/ORM/native/procedural call without #5559 edge |
| `sql_test_activation_unknown` | Registered pgTAP test with unknown activation |
| `sql_extension_version_unknown` | Extension required but version/availability not evidenced |

Unknown reach, database state, or policy effect is not `no_static_path`. Object-name
matching or migration co-membership cannot establish activation, reach, or
observation.

### Cross-language relations

Application, ORM, or native/procedural callers reaching SQL require registered
edges under RIPR-SPEC-0378 ([#5559](https://github.com/EffortlessMetrics/ripr-swarm/issues/5559)).
Co-located SQL strings, shared migration folders, or matching table names alone
are insufficient.

### Authority boundary

Initial PostgreSQL/pgTAP support is preview/advisory per
dialect/version/project/migration/framework/mode. No live database connection,
default feature, gate, badge, baseline, RIPR Zero, release, extension/package
publication, or migration authority from this spec alone.

### Capability cell registry (normative preview index)

Capability cells use identity shape
`{language}/{framework}/{mode}/{family}` per
[RIPR-SPEC-0365](RIPR-SPEC-0365-capability-manifest-cell-join.md). The table
registers every initial SQL cell for docs/traceability/manifest joins; runtime
registry entries land in later implementation slices under #5602.

| Cell id | Support state (initial) | Tier ceiling |
| --- | --- | --- |
| `sql/postgresql/diff/static_facts` | `not_proven` | `preview` |
| `sql/postgresql/repo/static_facts` | `not_proven` | `preview` |
| `sql/pgtap/diff/static_facts` | `not_proven` | `preview` |
| `sql/plpgsql/diff/static_facts` | `not_proven` | `preview` |
| `sql/flyway/diff/static_facts` | `not_proven` | `preview` |
| `sql/sqitch/diff/static_facts` | `not_proven` | `preview` |
| `sql/liquibase/diff/static_facts` | `not_proven` | `preview` |
| `sql/any/diff/repair_routing` | `not_proven` | `preview` |
| `sql/sqlite/diff/static_facts` | `not_admitted` | `preview` |
| `sql/mysql/diff/static_facts` | `not_admitted` | `preview` |
| `sql/ansi-generic/diff/static_facts` | `not_admitted` | `preview` |

Product manifest claims for admitted ids are added only when
`metrics/capabilities.toml` entries explicitly cite implementation proof; this
spec registers the ids for index completeness only. `not_admitted` cells exist
so traceability and docs explicitly record non-inheritance.

## Required Evidence

- `cargo xtask check-spec-format`, `check-spec-numbering`, `check-doc-artifacts`,
  and `check-traceability` pass with this spec and its index registrations.
- Acceptance examples in this document are keyed for future SQL judged corpora
  under #5602 as `RIPR-SPEC-0404#<slug>`.
- Implementation slices under #5602 must add executable proof; this document
  alone does not satisfy runtime, adapter, or database evidence.

## Non-Goals

- No database/catalog/migration/query/pgTAP/application execution during passive
  analysis.
- No universal SQL dialect or live-catalog reconstruction from repository files.
- No adapter code, routing, feature gates, default enablement, or support-tier
  promotion.
- No generated tests, source edits, release, or publication actions.

## Acceptance Examples

1. **dialect-project-positive**: A repo with `flyway.conf` and
   `sql/migrations/V1__init.sql` tagged `role = migration`, `framework = flyway`,
   `dialect = postgresql` with declared `server_version_range`, yields substrate
   facts separate from semantic probes on query files.
2. **dialect-project-negative-ansi**: A lone `queries/report.sql` without dialect
   evidence MUST emit `dialect: unknown` and MUST NOT claim PostgreSQL or ANSI
   support from extension alone.
3. **non-inheritance-sqlite**: A `.sql` file with SQLite-specific syntax MUST NOT
   inherit `sql/postgresql/*` capability rows; `sql/sqlite/diff/static_facts`
   remains `not_admitted`.
4. **schema-qualification-positive**: `CREATE TABLE auth.users (...)` records
   schema `auth`, quoted identifier `"Users"` preserves case in identity; probe
   spans cover the full qualified name.
5. **search-path-negative**: `SELECT * FROM accounts` with no static `search_path`
   MUST name `sql_search_path_unresolved` rather than guessing `public.accounts`.
6. **overload-signature-negative**: Two `CREATE FUNCTION foo(int)` / `foo(text)`
   overloads changed on one parameter MUST NOT credit the other without signature
   match; unresolved sets name `sql_overload_unresolved`.
7. **migration-order-positive**: Editing `V2__add.sql` versus adding `V3__add.sql`
   yields distinct `migration_id` movements; reordering versions changes
   `lineage_order` without collapsing prior migrations into a snapshot schema.
8. **migration-timeless-negative**: A folder of DDL files without order metadata
   MUST NOT synthesize a single canonical schema; `sql_migration_state_unknown`
   applies.
9. **external-state-negative**: Referencing a dump `schema.sql` tagged
   `producer = pg_dump` does not set `live_catalog_state = known` without a linked
   receipt in disposition document.
10. **pgtap-plan-distinct**: `SELECT plan(5);` in a test file registers harness
    control only; no oracle credit on production tables from `plan` alone.
11. **pgtap-activation-positive**: `SELECT ok(1=1, 'sanity');` with
    `activation = active` and exact assertion span may carry scalar oracle facts
    after #5542 activation checks pass in implementation.
12. **pgtap-skip-todo-negative**: `SELECT skip('reason');` or todo-marked tests
    keep `activation = skipped` / `todo` and remain oracle-ineligible.
13. **pgtap-dynamic-negative**: Procedural generated test names keep
    `activation = dynamic_unknown`; no oracle from description string alone.
14. **oracle-rowcount-vs-values**: `SELECT results_eq('SELECT count(*)', ...)` and
    `results_eq` on row values MUST remain distinct oracle families; row-count
    credit does not grip value mutations.
15. **oracle-order-negative**: `SELECT set_eq(...)` without `ORDER BY` in the
    compared query MUST NOT treat order as discriminating unless ordering oracle
    explicitly established.
16. **result-shape-vs-metadata**: Column existence assertions (`has_column`) MUST
    NOT credit changed SELECT expressions in unrelated queries.
17. **error-sqlstate-positive**: `throws_ok(...)` maps to SQLSTATE/message spans
    when statically literal; unrelated `RAISE` in functions stays separate.
18. **transaction-setup-positive**: `BEGIN; ... ROLLBACK;` setup blocks tag
    `transaction_context` separately from assertion spans; postconditions cite
    exact statements only.
19. **rls-dynamic-limits**: `ENABLE ROW LEVEL SECURITY` declarations name policy
    objects; runtime policy effect without static proof names
    `sql_rls_or_policy_unresolved`.
20. **dynamic-sql-limit**: `EXECUTE format('SELECT %I', col)` names
    `sql_dynamic_sql_unresolved` for reach through dynamic text.
21. **trigger-cascade-limit**: `CREATE TRIGGER ...` links trigger identity; cascade
    behavior without static path names `sql_trigger_or_rule_unresolved`.
22. **cross-language-negative**: Ruby model referencing table name `users` without
    #5559 edge MUST NOT relate to SQL migration defining `users`.
23. **test-activation-before-oracle**: Ineligible activation suppresses all oracle
    families even when assertion text matches a changed function.
24. **exact-span-multi-expr**: Two comparisons on one line receive two probe ids
    with non-overlapping spans.
25. **capability-index-complete**: Every row in the capability table appears in
    traceability metrics `sql_capability_cell_index_complete` registration.

## Test Mapping

See `.ripr/traceability.d/5736-postgresql-pgtap-evidence-contracts.toml`. Executable
tests land with analysis slices under #5602. Until then,
`fixtures/*/verdict-corpus/spec-scopes/RIPR-SPEC-0404.toml` marks the spec out of
runtime mutant corpora.

## Implementation Mapping

Follow the #5602 child graph after this contract lands:

- PostgreSQL parser/catalog/fact-substrate ADR and routing/config/dialect discovery
- Schema objects, migrations, queries, probes, stable IDs
- pgTAP activation, plan, setup, and oracle emission
- Result-set/schema/error/state/transaction discriminator model
- Dependency/dynamic-SQL/search_path/trigger/RLS/extension limits
- Bounded repo mode and migration/schema completeness
- Application/procedural cross-language edges through #5559
- Output/editor/agent parity and judged corpora/support-tier decision

## Metrics

- `sql_evidence_contract_registered`
- `sql_capability_cell_index_complete`

## Failure Modes

- Treating `.sql` as dialect-selected without project/configuration evidence.
- Collapsing migration sequences into a timeless schema or executing migrations
  passively to infer state.
- Crediting pgTAP registration/planning functions as oracles or production reach.
- Promoting preview SQL findings to gate or badge eligibility without promotion
  policy and proof under #5602.
- Inferring live database rows, applied migrations, or policy effects from
  repository files alone.
