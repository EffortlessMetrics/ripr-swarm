# Returned rendering buffer evidence (#3161)

This is a reusable support-boundary fixture, not a classification repair.
`returned_render_buffer.rs` is a dependency-free Rust 2024 test crate and is
also compiled as the `native` module of the integration test of the same name.
The public-API test isolates each observer in its own temporary crate, retains
one changed helper call and requires a nonempty, unique call-deletion finding.
No receipt, file, process, JSON, CLI or general interprocedural transfer is added.

## Source and order

The subject reduces UB-review head
`b97b2edefe0b41a23e0b5e081a39a60849071b0f`, tree
`15eb5877b9a2f5f373b93892e65849995c8af037`:

```text
rendered_summary: text = String::new()
  -> text.push_str("before\n")
  -> append_diagnostic(&mut text)
       -> [changed] render_pr_signal(text, exact diagnostic)
            -> trim -> format! -> text.push_str
  -> tail returns the same text
  -> test binds rendered
  -> rendered.contains(exact diagnostic)
```

The later-write case inserts `text.push_str("after\n")` after the helper and
before returning the same local. No statement is moved to manufacture a final
helper-call shape. The native controls preserve this order. The full UB caller
also has loops, conditionals and `cap_review_body(text, limit)`; this fixture does
not establish their identity, activation or output survival. The sentence and
Markdown transformations are reduced to `trim` and a bullet format. The changed
call stays in a helper, rather than being moved into the tested wrapper.

## Existing authority and first missing representation

At analyzer c96a7929f8c694eb62cff2dbe2ad7ee94beea815:

- Merged #7355 / SPEC-0159 retains partial identity for a field constructed
  directly inside a record passed to `push` on a mutable parameter. Its
  `ReturnedLedger` records parameter/local/caller, checks parsed mutability,
  scope and binding uniqueness, and requires the helper call to be the final
  statement before the exact returned-local tail. The field/element/observer
  edges remain unresolved. A higher wrapper's discarded result still refuses.
- `returned_ledger` admits only `FieldConstruction`; this changed expression is
  `CallDeletion`, a free helper call. There is no record field or Vec element.
- Direct collection observation admits bare-receiver `push`. `push_str`, the
  free helper's mutable argument, and the returned caller-local String are not
  supplied by that rule. `EffectStateCarrier` addresses `self.method` fields,
  not this path. Existing helper result forwarding cannot establish an effect
  that returns unit but escapes through a mutable argument.
- Substring assertions are relational/weak. The identical literal in a sibling
  buffer is not the changed effect's observer. Oracle strength alone cannot
  establish this missing identity or prove sensitivity to call deletion.

The smallest candidate prerequisite is a producer-owned **partial** path in the
existing witness, reusing the parsed binding/uniqueness authorities:

```text
changed helper-call occurrence + resolved target
  -> exact argument position and &mut String owner parameter binding
  -> nested writer receiver (candidate until String/std identity is established)
  -> exact caller &mut local argument at the call site
  -> unshadowed/unreassigned returned local, with statement order retained
  -> test's exact wrapper invocation and bound result
  -> assertion's primary receiver is that result
```

There is no new DTO or producer in this evidence PR. The current edge vocabulary
is `DirectReturn`, `ErrorVariant`, `StructField`, and `EffectTarget`.
`retain_returned_ledger` already uses candidate/established `EffectTarget` edges
for sink-to-parameter and parameter-to-caller-local, then an established
`DirectReturn` edge for that local. These endpoints can retain part of the new
identity route; they do not supply a call-argument position/order receipt or a
general observer binding. `selected_helper_calls` supplies identity-only inherent
call receipts, not free mutable-argument transfer; `fallible_check` is a separate
rule-7a port. A smallest successor slice could extract the parser-backed
parameter-to-returned-local identity checks from `returned_ledger` and retain a
partial free-call receipt without changing classification. That requires exact
call occurrence/argument and binding/order evidence for this new family, rather
than widening the field/push gate by method name. The later-write positive
would still need its own ordered survival rule; blanket refusal is honest until
that rule exists. A subsequent observer slice must name the asserted receiver
and refuse all orthogonal/replaced-result controls before any promotion. It must
retain a typed first-unresolved edge instead of using method spelling or the
right literal as authority. Native deletion receipts are independent evidence,
not input to static classification. Even a complete local identity path does
not imply exact-string mutation discrimination: `contains` may tolerate other
changes, and a preexisting diagnostic or another writer may survive deletion.

## Falsifiers

| Case | Native diagnostic-call deletion | Missing/static obligation |
| --- | --- | --- |
| Same returned local observed | Assertion fails | Bind helper write to returned local and asserted receiver |
| Same local with later append | Assertion fails | Preserve write order; do not silently move helper to final statement |
| Sibling-only oracle | Passes | Same text on another buffer grants no credit |
| Ignored return | Passes | Calling wrapper without observing its result grants no credit |
| Buffer shadowed after helper | Passes | Return points to a different binding |
| Wrong helper writes same text | Passes | Resolve changed helper identity; literal coincidence is insufficient |
| Buffer cleared and rewritten | Passes | Track destructive later writes; return identity alone is insufficient |

The public-API matrix has corresponding isolated cases, with shadow/overwrite
in the returning wrapper and a wrong-helper returning wrapper. All remain
unconfirmed; no class is upgraded. These are intentional non-support pins that
a future producer change may revise only with aligned positive and negative
witnesses. They do not add a new support-tier or spec acceptance claim. Existing
SPEC-0159 behavior remains covered by its own mapped helper-wrapper suite;
these additional limitation experiments intentionally have no new traceability
acceptance mapping.

## Reproduce

```sh
cargo test -p ripr --test returned_render_buffer
rustc --edition=2024 --test crates/ripr/tests/data/returned_render_buffer.rs -o <owned-temp-binary>
<owned-temp-binary>
```

In an isolated copy, delete only the complete `render_pr_signal` statement in
`append_diagnostic`; compile and execute again. Expected: two actual-buffer
observers fail, five decoys pass. A compile/setup error is not that result.
No mutation command edits UB or the submitted candidate.

Rollback removes these three test/evidence files. No runtime product, schema,
policy, gate, pin, suppression, Ready, merge or consumer-clearance change.
