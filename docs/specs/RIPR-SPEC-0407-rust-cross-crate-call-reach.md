# RIPR-SPEC-0407: A call spelled through a workspace dependency carries reach and the boundary input across the crate edge

Status: proposed

Owner: product / analysis

Created: 2026-11-02

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #11401

Linked PRs:

- None yet

Support-tier impact:

- No tier change. Rust static analysis stays advisory. A change can now read
  `exposed` through one more call shape; nothing is claimed beyond what the
  same-crate chain already claims, and no runtime outcome is claimed.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `.ripr/traceability.toml` and
  `policy/doc-artifacts.toml`.
- No output schema change.

## Problem

Workspace `wscore` holds `pub fn discount(price)` and `app` holds
`pub fn checkout(price) -> u32 { wscore::discount(price) }`. The only test is
`app/tests/t.rs`: `assert_eq!(checkout(100), 90)`. When `discount`'s
`price > 100` becomes `price >= 100`, the test passes the boundary input 100
straight through and pins the result exactly, so it notices the change. ripr
read it as `no_static_path` with `transitive_reach_unresolved`, for two
separate reasons:

1. The helper-transfer chain (RIPR-SPEC-0159) binds a test's argument down a
   chain of same-workspace calls and pairs an exact pin on the entry's result
   with the boundary. It knew calls spelled by bare name and calls spelled
   through an own-crate module path (#8999). A call spelled through another
   crate (`wscore::discount(..)`) was "not statically splittable".
2. In a virtual workspace the root manifest has no package of its own, so the
   index knew no member crate name. `use app::checkout;` in `app/tests` then
   read as an import from a foreign crate, and the test's call of the chain
   entry was never recognised in any member package. A wrapper and an owner
   inside one member read `weak` for the same reason.

## Behavior

### The cross-crate hop

A function `caller` calling the one function `callee` of the workspace is a
chain hop when every link is exact:

1. `callee` is the only function of its name in a workspace-complete index
   (the existing chain rule), a body-carrying free `fn` declared `pub`
   (unrestricted) at the top level of a file, outside every module block and
   impl.
2. The manifests give exactly one name for `callee`'s library in `caller`'s
   package (`MemberCrates::library_dependency_name`): `callee` is in the
   library root file of its package, the two packages differ, and the
   caller's `[dependencies]` (or `[target.*.dependencies]`) holds a path
   entry on the callee's package. The name is the entry's key: the callee's
   own library identifier, or the new name a `package = ".."` rename gives it
   (`disc = { path = "../wscore", package = "wscore" }` names the crate
   `disc`). Two entries naming the package (`wscore` and `disc`), or a key
   that another entry binds to a different package, are ambiguous and give no
   name. A `[dev-dependencies]` or `[build-dependencies]` entry is not
   visible to library code and gives no name. Registry
   and git entries, `[patch]` and `[replace]` of the name, Cargo config
   mentions and any unreadable or unexpected manifest give no name, exactly as
   they refuse the import authority this extends.
3. `caller` is production code (not a test, not under `tests/`) and mentions
   `callee` once, in one call fact, spelled `<crate>::<callee>(..)` with
   exactly that one qualifier segment: no method call, longer path, leading
   `::` or generic argument. The one other spelling is a bare `callee(..)`
   after an import by name, below.
4. Nothing can rebind `<crate>`. In the caller's own file there is no `use`
   of the name, alias, glob, block-local item, or macro or `include` that
   could splice an item in. In every other file of the caller's package there
   is no textual `mod <crate>` declaration; other forms of rebinding in those
   files are not scanned.
5. `caller` invokes no macro that could hide an early return or a rebinding.

The hop is `module_qualified` (so every consumer without a cross-crate proof
keeps refusing it) and carries `crate_qualifier`. Only the tail-forwarding
rule reads that name: the hop forwards the callee's result when the caller's
tail is exactly `<crate>::<callee>(..)` with no early `return`, no `?` and no
other shape (`+ 1`, a bound-then-shifted result, a branch). Argument binding
down the hop is the existing positional binder, so an argument the caller
computes (`price + 1`) binds nothing and the boundary input is not credited.

A bare call between two different member packages is a hop only through the
import below; otherwise it is never a hop (the root package keeps its
existing reading), so the member-package entry fix below cannot expose it by
accident.

### A bare call after an import by name

`use wscore::discount;` and `use wscore::{discount, ..}` (also
`use wscore::{self, discount}`) make a bare `discount(price)` the same hop,
under the same manifest name (renamed included: `use disc::discount;`), when
all of these hold in addition to rules 1, 2 and 5 above:

- `caller` is a top-level function of its file, and the bare name appears
  exactly once in its body, in one direct call;
- exactly one `use` in the file mentions the callee name. It is a top-level
  `use` with no attribute (`#[cfg(..)]`) and no visibility (`pub use`), whose
  path is `<crate>::<callee>` or whose list holds the bare `callee` under
  `<crate>`. An alias (`as d`), a glob, a longer path
  (`wscore::pricing::discount`), a leading `::`, an import of the name from
  another crate, an import inside a function or module, and a second import
  of the name all refuse;
- the file declares nothing named `callee` (function, binding, parameter,
  closure parameter, item or macro) and holds no `extern crate`, `include` or
  macro that could splice items in;
- no file of the caller's package declares `mod <crate>`.

A glob import (`use wscore::*;`) never names the callee, so a bare call after
only a glob stays unreached. The tail rule accepts the bare call as the whole
tail for such a hop; the single admitted mention is what is checked.

### A test of the entry's own package

A test in `app/tests` that calls the chain entry `checkout` after
`use app::checkout;` is not a foreign import when `app` is the library name
the manifests give for the entry's own package
(`MemberCrates::import_names` for the test file and the entry's library root).
A test of another package, an entry outside a library root and an unreadable
manifest add nothing, so those imports still read foreign.

### A direct test beside the cross-crate entry

An owner can also have a direct test in its own package (`discount(5)` in
`wscore/tests`) beside the wrapper test in `app`. The multi-caller rule counted
that direct test as a second reached caller and gave up the chain, and the
activation rows preferred the direct call over the chain's. When the owner has
exactly one non-test caller reached by a test, and its chain crosses a crate
edge (a hop carries `crate_qualifier`), that chain stands, and its transferred
rows are added to the direct rows. Both sides stay as they were: the direct
test keeps its own relation and rows, and the pairing still needs a boundary
row and an exact pin on the same entry call in one test. Any other mix of
callers stays ambiguous. A chain with no crate edge is unchanged.

### What the result is

With the hop and the entry recognised, the existing rules decide, unchanged:
the test relates as `helper_owner_call`; activation binds the test's literal
down the hop; the boundary pairing needs a transferred boundary row on the
entry call and a forwarding chain; and the oracle must be an exact value pin.
The audit case reads `exposed`. A transformed argument, a transformed or
re-bound result, a loose assertion and an input off the boundary read as they
do for a same-crate wrapper: not noticed.

## Bare `ripr repair`

Bare repair reads whole-repository seams from its own producer, which relates
tests within one package by name and does not follow a call into another
crate. For a changed spot it therefore asks `ripr check`, the authority for a
changed line, through the same analysis a default `ripr check` runs. When a
seam on the changed lines binds (shared location join: file, owner, probe
family and line, most-specific claimant only) to a finding that check reads as
`exposed`, the seam's refusal becomes "already noticed":

> `ripr check` reads this changed code as already noticed by a test, so there
> is no gap to repair here

This only ever closes a start. A seam is never made startable, a seam off the
changed lines is not consulted, and a check that cannot run keeps every
refusal as the seam producer made it.

The check analysis is expensive on a large repository, so it is bounded. It
runs at most once per `resolve_repair_start` call, which both bare `ripr
repair` and the `ripr continue` start suggestion use, and only when an open
seam on the changed lines passes a cheap pre-test: there is a `Cargo.toml`
besides the owner's own (path-only scan; a virtual workspace root counts, and
so does a nested manifest such as `fuzz/Cargo.toml`), and the owner is a
top-level `pub fn` of its package's `src/lib.rs`, the shape another crate can
name as `<crate>::<fn>`. A repository whose only manifest is the owner's own
never runs it, and an unreadable owner file counts as passing. Refusals the pre-test excludes (a
private owner, a method, a same-crate wrapper) keep the seam producer's
answer.

Repair takes no check mode, so it reads the whole workspace like the default
check does. In `--no-unchanged-tests` and `--mode instant`, check leaves the
unchanged `app` test out, prints "no test path" and offers no start, while
repair names the noticed change; neither starts. An explicit `ripr repair
<id>` start goes through the seam gate alone and is unchanged.

## Known limits

- A `[target.'cfg(..)'.dependencies]` entry counts as a dependency edge
  whatever the platform. A call that only builds on another platform would be
  a build error there, so this is not a realistic source of a wrong reading.
- The seam producer itself still does not follow a call into another crate;
  repair gets the answer from check for the changed lines only. A seam off the
  changed lines, and an explicit `ripr repair <id>` start, use the seam
  producer alone.
- Repair runs the default check analysis a second time only under the
  pre-test above; a changed method or non-`pub` function, or a function outside
  a package's `src/lib.rs`, is outside it, so repair keeps the seam producer's
  answer there.
- `--no-unchanged-tests` and `--mode instant` leave the unchanged `app` test
  out of check's evidence, so check keeps the earlier no-path reading there
  (repair, which has no mode, reads the workspace whole).
- Not followed: a glob import, an aliased or nested import, a submodule path (`wscore::pricing::discount`), a method or associated
  function, more than one crate edge in a chain beyond what the existing
  chain bound allows, and a return-value probe whose exact pin needs the
  same-crate identity proof (the hop is `module_qualified`).
- Static evidence only. Real mutation testing remains a later authority.

## Required Evidence

- Real `ripr check` and bare `ripr repair` on generated git workspaces, one
  negative per boundary that changes a single thing from the positive and
  keeps the same changed owner and line, so no negative passes for another
  reason.
- A failing-first run of the positive and the member-package case on `main`
  without this change.
- The existing helper-transfer, related-test, activation and boundary-pairing
  unit tests unchanged and passing.

## Non-Goals

- Teaching the seam producer to follow a call into another crate.
- Resolving a bare `use`d function, a renamed dependency, a submodule path or
  a method across a crate edge.
- Any runtime claim, test generation or change to the actionability flip.

## Acceptance Examples

1. `wscore::discount` changes `price > 100` to `price >= 100`;
   `app::checkout` returns `wscore::discount(price)` through a normal
   `[dependencies]` path entry; `app/tests/t.rs` asserts
   `assert_eq!(checkout(100), 90)`. `ripr check` reads the change as noticed
   and offers no repair start; bare `ripr repair` does not start and says the
   change is already noticed.
2. The same workspace with `wscore::discount(price + 1)`, with
   `wscore::discount(price) + 1` (and the pin moved to 91), with
   `assert!(checkout(100) > 0)`, or with `checkout(150)` is not read as
   noticed.
3. The same workspace declared only under `[dev-dependencies]`, shadowed by a
   local `mod wscore`, with the call still spelled `wscore::` after a rename,
   with the package reachable under two names, with a rename that points at
   another package, imported by glob (`use wscore::*;`) or by alias, imported
   inside the function or under `#[cfg]`, shadowed by a local `fn discount`
   or binding, or with a second `discount` in a third crate keeps the earlier
   no-path reading.
3a. The same workspace with `disc = { path = "../wscore", package = "wscore" }`
   and `disc::discount(price)`, or with `use wscore::discount;` /
   `use wscore::{self, discount};` / `use disc::discount;` and a bare
   `discount(price)`, reads as noticed like example 1, and bare repair does not
   start. With `discount(price + 1)` after the import, or the pin moved to
   `checkout(150)`, it is not read as noticed.
4. With an extra `wscore/tests/other.rs` asserting `discount(5) == 5`, check
   still reads the change as noticed, and bare repair does not start. With the
   `app` pin moved to `checkout(150)`, loosened to `assert!(checkout(100) > 0)`,
   the pin loosened under a renamed dependency or the argument changed, check reads a partial gap
   and both commands start the same repair.
5. `checkout` and `discount` both in one member package whose integration test
   does `use wscore::checkout;` reads as noticed in a virtual workspace, and bare repair agrees (the virtual
   workspace root is a manifest besides `wscore`'s own and `discount` is a
   top-level `pub fn` of `wscore/src/lib.rs`, so the check run is made).

## Test Mapping

- `crates/ripr/tests/cross_crate_reach.rs`.
- `crates/ripr/tests/check_repair_agree.rs`
  (`unreached_workspace_change_is_not_called_gripped_by_repair` uses a
  `[dev-dependencies]`-only edge as its unreached fixture).
- `crates/ripr/src/analysis/facts/member_crates.rs` (`library_dependency_name`
  tests) and `crates/ripr/src/analysis/classify/helper_transfer/cross_crate_hop.rs`
  (declaration, call-shape and module-name tests).

## Implementation Mapping

- `crates/ripr/src/analysis/facts/member_crates.rs`:
  `MemberCrates::library_dependency_name`, the `[dependencies]`-only,
  cross-package variant of the manifest import authority; a `package` rename
  is named by its key.
- `crates/ripr/src/analysis/classify/helper_transfer/cross_crate_hop.rs`:
  `cross_crate_free_hop`, reached from `chain_through_caller` after the
  own-crate qualified route. `HelperHop::crate_qualifier` carries the crate
  name; `hops_forward_owner_result_with` reads it for the tail rule.
  `cross_crate_imported_free_hop` is the bare-call-after-import variant,
  reached from `chain_through_caller` where a bare call between two member
  packages was refused.
- `crates/ripr/src/app/task_first.rs`: `notice_what_check_notices`, called by
  `resolve_repair_start` between the shared gate (`repair_refusal`) and the
  refusal presentation; the binding is
  `repair_card_handoff::finding_names_seam_by_location`.
- `crates/ripr/src/analysis/classify/helper_transfer.rs`:
  `resolve_multi_caller` (a direct test beside a cross-crate entry) and
  `test_call_is_shadowed_for_entry` (the member-package import fix, read by
  `related_tests` and the activation entry rows) and
  `crosses_member_packages` (a bare call between two members is not a hop).
- `crates/ripr/src/analysis/classify/activation.rs`: `call_values_for_owner`
  adds the chain's transferred rows to the direct rows when the chain crosses a
  crate edge.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
