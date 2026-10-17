# RIPR-SPEC-0279: Rust doc examples as tests

Status: proposed

Owner: product / analysis

Created: 2026-10-15

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #6648 (code pinned only by a doc example reads as untested)

Linked PRs:

- None yet

Support-tier impact:

- No tier change. A doc example becomes one more Rust test shape on the
  existing related-test and owner-return pin paths.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No schema version bump. No new oracle kind, strength, class, relation
  reason or limitation kind. A registered doc example is a `TestFact` whose
  name is rustdoc's display name (`src/lib.rs - read_u16 (line 8)`).
- Narrows RIPR-SPEC-0231's owner-return pin for this test shape only: the
  assertion gate reads the example's own source, and call identity names
  the owner only through the library's import name.

## Problem

`cargo test` compiles each runnable fenced example in a library's `///`
comments as its own crate that links the library, and runs it. Code that
only such an example pins is tested, and real mutants of it fail
`cargo test`:

```rust
/// Reads a big-endian `u16` from the first two bytes.
///
/// ```
/// assert_eq!(wire::read_u16(&[0x12, 0x34]), 0x1234);
/// ```
pub fn read_u16(bytes: &[u8]) -> u16 {
    u16::from_be_bytes([bytes[0], bytes[1]])
}
```

ripr indexed no test for the example, so the return value of `read_u16`
read as a gap (verdict-corpus row `mined-doctest-only-read-u16`,
`false_actionable`). Crates such as `bytes` pin most of their accessors
only this way.

## Behavior

### Rule 1: which doc examples are tests

A fenced block in a `///` comment (not `////`, not `//!`, not
`#[doc = ".."]`) is registered as a test of the file that holds it when
every condition holds; anything else registers nothing.

- The file composes under its package's default `src/lib.rs` library root
  through resolved `mod` edges only (no `include!`, no unresolved or
  test-only edge), and the manifest keeps that root (`autolib` on, no moved
  `[lib] path`), leaves `[lib] doctest` on, and builds a Rust library a
  doctest can link (`lib`, `rlib`, `proc-macro`, or no `crate-type`).
  Without manifest authority, nothing registers.
- The comment documents a public production function of that file: the
  first item line after it is `pub` and function qualifiers, then `fn`
  and the function's name. An item inside a `#[cfg(test)]` module, a test,
  a non-function item, a `pub(crate)` or private function (an example
  calling it from outside the crate cannot compile), or a statement in a
  function body registers nothing. Privacy of enclosing modules is not
  checked.
- The comment holds no `<!--`, no `~~~` line and no fence indented four or
  more spaces or by a tab. In those, CommonMark may read the backticks as
  an HTML comment, as text inside another fence, or as an indented code
  block, which rustdoc does not run as written; the scanner does not model
  Markdown blocks, so the whole comment registers nothing.
- The fence's info string is empty or names only `rust` or an edition
  (`edition2015` to `edition2024`). `ignore`, `no_run`, `compile_fail`,
  `should_panic`, `text` and every other word refuse: rustdoc does not run
  such a block's assertions as written, or does not compile it as Rust.
- The block declares no `fn main` (rustdoc then runs the code unwrapped),
  is closed, and its code parses.

The registered test's source is synthetic: the example's code, with
hidden `# ` lines restored and `##` read as `#`, inside
`#[test] fn ripr_doctest() { .. }` whose lines are the comment's lines, so
every call and assertion fact points at the line the user wrote. Its name
follows rustdoc's display name, `<path> - <item> (line <fence line>)`, with
the bare function name as `<item>`: rustdoc qualifies a function in a
submodule (`src/codec.rs - codec::read_u16 (line 3)`), and this name does
not. It is a label, not a `cargo test --exact` filter.

### Rule 2: assertions are read from the example

The owner-return pin's assertion gate (RIPR-SPEC-0231) reads a doc
example's assertions from its synthetic source under the synthetic name,
never from the file that holds the comment, where no such function exists.
On top of the workspace's and the holding file's possible macro rebindings
(kept, conservatively), every binding site in the example itself (a
`macro_rules!`, an import, a glob, a drop-in crate) makes the names it may
bind ambiguous; no module of the example resolves and no drop-in crate is
verified for it. A `std::`/`core::`-rooted `assert_eq!` anywhere in the
example refuses.

### Rule 3: the example reaches the owner through the crate name

A doc example is its own crate. Its call names the owner only as a free
call through an import name of the owner's library from the example's
package (`[lib] name`, else the package name with `-` read as `_`):

- `wire::read_u16(..)` (or a longer module path from the library root)
  reaches the owner on the RIPR-SPEC-0231 path rules, with the example's
  source in place of the test crate: a `mod`, type, trait, alias, `use`
  binding of the name, or any glob in the example refuses, as does a
  rename to the name anywhere in the workspace.
- A bare `read_u16(..)` reaches the owner only through the example's own
  `use`: exactly one binding `use wire::..::read_u16;` and no glob, or only
  globs `use wire::..::*;` that each deliver the owner. A twin item of the
  name in the example, a rename to it, or no `use` at all refuses.
- `crate::`, `self::` and `super::` name the example's crate, never the
  library: a path or `use` rooted there refuses.
- A method or associated call, and an owner with same-named rivals (whose
  identity is settled in the holding file's scopes), refuse.

### Decisions

1. **The example is its own crate.** Adopted: scopes, macro bindings and
   call identity come from the synthetic source. Rejected: reading the
   holding file, where `crate::read_u16` would resolve to the owner and
   the module's `use` items would be in scope; neither is true in the
   compiled example.
2. **Fail closed on fences.** Adopted: an allow-list of fence words.
   Rejected: a deny-list of known modes, which would admit a block in an
   unknown language or mode as a running test.
3. **Free calls only.** Adopted: method, associated and rival-owner calls
   refuse. Typing a receiver or telling rivals apart reads file scopes
   that are not the example's.

## Required Evidence

- Unit tests pin the fence allow-list, hidden-line restoration, the `fn
  main` refusal, the target and manifest refusals, the test-module refusal
  with its control, and that a synthetic source answers only for its
  registered test.
- Public-API tests pin rule 3's admitted forms and each refusal against a
  control that pins.
- On the Rust verdict corpus, `mined-doctest-ignored-read-u16-le` does not
  move and no row becomes `false_exposed`. `strsim-sorensen-dice-equal`
  moves from `false_actionable` to `ideal`: `sorensen_dice`'s doc example
  (`use strsim::sorensen_dice;`, `assert_eq!(0.0, sorensen_dice("", "a"))`)
  is the discriminator the runtime truth records. `mined-doctest-only-read-u16`
  reaches the pin, and its `header_case!` wrapper no longer makes
  `assert_eq!` ambiguous (#8113). The row stays `false_actionable` only
  because its `call_deletion` finding on the deleted return tail is still
  a gap, which is a separate rule.

## Non-Goals

- `//!` crate-level examples, `#[doc = ".."]` attributes, examples on
  structs, traits, modules and other non-function items, and examples in
  binaries or moved library roots.
- Examples with `fn main`, and fences rustdoc runs with a changed outcome
  (`should_panic`) or not at all (`no_run`, `ignore`).
- Method, associated and trait calls from an example; owner identity among
  same-named rivals.
- A runtime claim: a registered example is static evidence that a test
  appears to discriminate, not a claim that it compiles or passes.

## Acceptance Examples

1. The `read_u16` example above, with the diff rewriting its body: the
   return value reads `exposed`.
2. As example 1 with `use wire::read_u16;` (or `# use wire::read_u16;`, or
   `use wire::*;`) and a bare `read_u16(..)`: `exposed`.
3. As example 1 under ```` ```ignore ````, ```` ```no_run ````,
   ```` ```should_panic ````, ```` ```compile_fail ```` or
   ```` ```text ````: no test is registered; the return value is not
   `exposed`.
4. As example 1 with `crate::read_u16(..)`, `self::read_u16(..)`,
   `use crate::read_u16;` or a bare call with no `use`: not `exposed`.
5. As example 1 with a local `mod wire { .. }`, a local
   `fn read_u16`, `use std::convert::identity as read_u16;`, a local
   `macro_rules! assert_eq`, or `std::assert_eq!`: not `exposed`.
6. A method `Reader::read_u16` documented with
   `reader.read_u16(..)`: not `exposed`, while the same comparison in a
   unit test is.

## Test Mapping

- `crates/ripr/src/analysis/facts/doctests/tests.rs`: rule 1.
- `crates/ripr/tests/doctest_owner_pins.rs`: rules 2 and 3, examples 1 to 6.
- `fixtures/rust-verdict-corpus`: `mined-doctest-only-read-u16`,
  `mined-doctest-ignored-read-u16-le` and `strsim-sorensen-dice-equal`.

## Implementation Mapping

- `crates/ripr/src/analysis/facts/doctests.rs`: `register_doctests`,
  `doc_examples`, `DocExample::synthetic_source`.
- `crates/ripr/src/analysis/facts/member_crates.rs`:
  `MemberCrates::runs_library_doctests`.
- `crates/ripr/src/analysis/facts/index.rs`: `RustIndex::doctest_source`.
- `crates/ripr/src/analysis/classify/owner_pin.rs`:
  `OwnerPinSyntax::doctest_refusal`, `OwnerReturnPin::admits`,
  `FreeOwnerBinding::path_reaches_owner_uncached`,
  `FreeOwnerBinding::doctest_bare_call_reaches_owner`,
  `test_crate_binds_root`, `source_may_bind_root`.

## Metrics

- `rust_doc_example_mismatch`: acceptance examples whose observed reading
  differs from this spec. Target: zero.
