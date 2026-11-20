# RIPR-SPEC-0450: Owner Opt-Outs Honor Enclosing `#[mutants::skip]` Scopes

Status: accepted

Owner: product / swarm

Created: 2026-11-15

Linked issues:

- [#14135](https://github.com/EffortlessMetrics/ripr-swarm/issues/14135) -
  honor `#[mutants::skip]` on an impl block or module for the functions
  inside it.
- [#14133](https://github.com/EffortlessMetrics/ripr-swarm/issues/14133) -
  `ripr explain` says when a finding is withheld by an owner opt-out.
- [#13910](https://github.com/EffortlessMetrics/ripr-swarm/issues/13910) -
  parent: owner opt-outs across surfaces and placements.

## Problem

cargo-mutants mutates nothing inside a `#[mutants::skip]` function, `impl`
block, `trait` block or inline module, nor anything in a file whose first
item is `#![mutants::skip]`. `ripr check` read the attribute only on the
changed function itself (#13819), so a cargo-mutants codebase switching to
ripr saw findings for code it had deliberately opted out.

## Behavior

A Rust finding is withheld under reason `owner_mutants_skip` when its probe
owner, or a scope enclosing that owner, carries `#[mutants::skip]` or
`#[cfg_attr(<predicate>, mutants::skip)]` (the predicate is not evaluated,
as in cargo-mutants). The owner's own attributes decide first; otherwise the
innermost enclosing skipped scope decides. Scopes read: an enclosing `fn`,
`impl` (inherent or trait impl), `trait`, inline `mod`, and the file through
an inner `#![mutants::skip]`. Inner attributes inside an inline module, impl
or fn body count for that scope.

The withheld finding keeps its class and is disclosed with the scope that
carried the attribute: JSON `findings[].withheld_scope`
(`fn|impl|trait|module|file`, present only with `withheld_by`), and the
human, GitHub, missed-txt and triage outputs name the scope (for example
`scope: impl Cart`). `ripr explain <id>` in the default human format, on a
fresh run, starts with one line taken from the same run's decision, for
example ``Withheld: owner `limit` is inside impl `Cart` marked
#[mutants::skip] (owner_mutants_skip)``.

Fail-closed rules (a miss keeps the finding reported):

- only parser-backed attributes count; a file read by the lexical fallback,
  or one that does not parse cleanly, yields no enclosing scope;
- the owner is matched by name and `fn`-keyword line; zero or several
  candidates on that line yield no scope;
- an owner id shared by several functions is withheld only when every one of
  them opts out;
- a raw string, doc string or macro argument that spells `mutants::skip` is
  not the attribute.

## Required Evidence

- Built-binary `ripr check --format json` runs with a plain baseline, so a
  withheld finding is shown to keep its class and name its scope.
- Discriminating negatives: a same-named owner outside the skipped scope, a
  second impl of the same type, and a raw-string spelling keep reporting.
- Parser unit tests for scope order, inner attributes, an unparsable file
  and an ambiguous owner line.

## Non-Goals

- `#[mutants::skip] mod name;` is not followed into the module's own file.
- `const` and `static` item scopes.
- `explain --format agent` and `explain --from <artifact>` (the artifact
  records no opt-outs).
- LSP (#14136), MCP (#14138) and repo-scope (#14139) surfaces.

## Acceptance Examples

1. `#[mutants::skip] impl Cart { fn limit .. }` withholds `limit`'s findings
   with scope `impl`; a second `impl Cart` keeps reporting.
2. `#[mutants::skip] pub mod a { pub mod inner { fn limit .. } }` withholds
   `a::inner::limit` with scope `module`; `b::limit` keeps reporting.
3. `#![mutants::skip]` at the top of the file withholds with scope `file`.
4. A trait default method under a skipped trait is withheld with scope
   `trait`; a free fn outside it keeps reporting.
5. A fn declared inside a skipped fn is withheld with scope `fn`.
6. `#[cfg_attr(test, doc = r#"mutants::skip"#)]` on an impl withholds
   nothing.
7. `ripr explain` on a withheld finding starts with the `Withheld:` line; an
   ordinary finding prints none.

## Test Mapping

- `crates/ripr/tests/mutants_skip_owner_opt_out.rs::impl_level_mutants_skip_withholds_its_method_but_not_another_impl`
  and `::cfg_attr_impl_level_mutants_skip_withholds` - example 1.
- `crates/ripr/tests/mutants_skip_owner_opt_out.rs::module_level_mutants_skip_withholds_nested_fn_but_not_same_name_elsewhere`
  - example 2.
- `crates/ripr/tests/mutants_skip_owner_opt_out.rs::file_level_inner_mutants_skip_withholds`
  - example 3.
- `crates/ripr/tests/mutants_skip_owner_opt_out.rs::trait_level_mutants_skip_withholds_default_method_only`
  - example 4.
- `crates/ripr/tests/mutants_skip_owner_opt_out.rs::fn_nested_in_a_skipped_fn_is_withheld`
  - example 5.
- `crates/ripr/tests/mutants_skip_owner_opt_out.rs::raw_string_doc_on_impl_mentioning_mutants_skip_does_not_withhold`
  - example 6.
- `crates/ripr/tests/mutants_skip_owner_opt_out.rs::explain_states_owner_opt_out_withholding`
  - example 7.
- `crates/ripr/src/analysis/syntax/ra/owner_scopes.rs::tests::reads_impl_trait_module_and_file_scopes_innermost_first`,
  `::inner_attributes_are_rewritten_to_outer_form` and
  `::unparsable_source_and_ambiguous_owner_yield_nothing` - scope order,
  inner attributes and the fail-closed rules.

## Implementation Mapping

- `crates/ripr/src/analysis/syntax/ra/owner_scopes.rs::enclosing_scopes` -
  parser-backed ancestry.
- `crates/ripr/src/analysis/owner_opt_out.rs::rust_owner_opt_outs` - the
  decision and shared-id rule.
- `crates/ripr/src/output/human.rs::render_owner_opt_out_explain_line` and
  `crates/ripr/src/output/json/report.rs` - disclosure.

## Metrics

- No new metric.
