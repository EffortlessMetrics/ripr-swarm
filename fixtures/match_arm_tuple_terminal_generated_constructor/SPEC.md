# Fixture: match_arm_tuple_terminal_generated_constructor

Spec: RIPR-SPEC-0093

Corpus: RIPR-SPEC-0108 (`rust_terminal_generated_constructor_no_promotion`)

Issue: [ripr#1728](https://github.com/EffortlessMetrics/ripr/issues/1728)

## Given

A crate-root item macro emits a counterfeit `Some`. The unchanged full terminal
owner in `work_queue.rs` inherits it through `use crate::*`. The constructor
replaces the relation, so a compiling wrong-arm mutant passes all four native
projection assertions.

## When

The exact request-only arm diff is analyzed.

## Then

Both findings remain `weakly_exposed`, with `observation_unverified` for the
current arm. Opaque item expansions cannot establish constructor authority.

## Must Not

- Credit the literal erased by a generated constructor.
- Disable the real terminal owner or ordinary Option constructors.
- Rewrite the consumer or bless unrelated golden drift.
