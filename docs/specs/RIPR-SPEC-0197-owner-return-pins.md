# RIPR-SPEC-0197: Owner-return pins through calls that name the owner

Status: proposed

Owner:

Created: 2026-09-29

Linked proposal:

Linked ADRs:

Linked plan:

Linked issues:

- #4478 (confirm a return value pinned by `assert_eq!` on the owner's own call)
- #5027 (share bounded equality execution admission with ErrorPath and Predicate)
- #5040 (typed async-harness execution provenance; explicit unsupported boundary)
- #3727 (parser-backed call identity; this spec adds the owner's item
  container fact, not parser-derived `CallFact`)
- #6675 (a binary bitwise `|` tail is unconditional; closures and `||` stay refused)
- #6692 (a hand-written `Clone` field pinned by `assert_eq!(recv.clone(), recv)` through derived equality)
- #6957 (the owner's own enclosing module is not a shadow: nested
  production declarations keep their pin)
- #6950 (an out-of-line parent module declaring the receiver shadows it:
  the parent chain refuses the pin and direct reach)
- #7067 (a single-file `use ... as <name>` rename rebinds the receiver in
  either spelling: the raw form refuses the pin and direct reach too)
- RIPR-SPEC-0219 verdict corpus: `assert!(owner(..))` on a bool owner read
  as a weak relational check (bool-owner pins below)
- #6482 (an `assert_eq!` in a test-local check helper the test calls
  eagerly; rule 7)
- #7125 (the same helper shape in a `tests/*.rs` integration target)

Linked PRs:

Support-tier impact:

- No tier change. A Rust `return_value` finding can now read `exposed`
  when a related test pins the owner's whole return value through a call
  that names the owner. No new language or surface is claimed.
  [docs/status/SUPPORT_TIERS.md](../status/SUPPORT_TIERS.md)

Policy impact:

- None. No new CI gate or production process surface. The matched runtime integration
  test has one allowlisted test-only process-launch site for compiling/running the
  fixed fixture subjects.

## Problem

A Rust `return_value` probe confirms its discriminator only when an
assertion shares an identifier token with the changed expression. A test
that pins the owner's exact return value through a call of the owner shares
no such token. The OSS replay of tokio-rs/bytes 7930d93 changed
`Buf::try_get_int`'s tail to `Ok(sign_extend(self.try_get_uint(nbytes)?, nbytes))`.
The commit's own test `assert_eq!(a.try_get_int(3), Ok(-1))` kills every
mutant of that line, and ripr reported `weakly_exposed` with "Discriminator
unconfirmed".

A lexical "the assertion calls something named like the owner" rule is
wrong in three shapes, each of which an earlier candidate credited:

1. An associated function `T::name` next to a same-named free function
   `name`: a bare `name(..)` in a test names the free function.
2. A trait default method whose receiver's impl overrides it: the call runs
   the override, not the default.
3. An input that leaves the owner through another return path: with
   `Ok(g(x)? * 2)` changed, `assert_eq!(f(-1), Err(E))` observes the `?`
   exit and never evaluates the changed `Ok(..)`.

## Behavior

An `assert_eq!` confirms a `return_value` probe's discriminator when all of
the following hold. Each rule fails closed: when ripr cannot establish it,
the owner-specific pin is refused. Before either the pin or token matching
can supply any return-value oracle credit, bare `assert_eq!` assertions pass
the same execution/macro-context admission below. A refused context contributes
no observation, oracle kind, strength, or discriminator; its related-test row
remains visible with no oracle. If no admitted oracle remains, existing stage
combination produces `reachable_unrevealed` with Observe `no` and Discriminate
`no`. Here `no` means no statically established oracle, not proof that a test
can never execute. Other owner-specific pin failures retain the existing token
rule only for an assertion whose context was admitted.

1. Oracle shape. The assertion is one plain `assert_eq!` (never
   `assert_ne!`, `debug_assert_eq!`, a path-qualified or crate-specific
   `*_assert_eq!`) with an exact-value or whole-object oracle kind, on a line
   inside the test's own body (or borrowed from a check helper under
   rule 7), in a test without `#[should_panic]`. The
   parser must match the exact indexed function body and the assertion's
   line/text identity uniquely. Identical same-line invocations fail closed.
   The test itself must have item ancestry through modules/item lists to the
   source file, never an enclosing function, block, closure or impl. Module
   outer attributes and module/source-file inner attributes must establish
   availability in a test build. The existing `cfg_predicates` owner evaluates
   `test = true`, `all`/`any`/`not` and nested `cfg_attr`; `all()` is true and
   `any()` false. Feature/target/custom atoms remain unknown and cannot grant
   credit. Ordinary lint/doc attributes and definitely enabled forms such as
   `cfg(test)` or `any(test, feature = "unknown")` remain supported. Raw attribute
   heads (including raw introduced attributes) remain unestablished; refusing
   them does not alter the cached source-role classifier. Malformed
   operands are not silently discarded. Existing module-provenance edges bind
   out-of-line parents to their indexed source and declaration coordinate;
   missing, unresolved or include-only provenance cannot establish that context.
   The assertion must lie on an ordinary statement/block/initializer path,
   without conditional, async, const, labeled, nested-item, attributed-node
   or prior root-return context. Root returns after the actual assertion execution
   point do not defeat it. For a bound closure, use the invocation position, not
   its earlier definition; any closure return remains conservatively refused.
   A non-async zero-argument closure is supported
   only when immediately invoked, or when its immutable simple binding has
   exactly one reference in the entire function: a subsequent zero-argument
   call in the binding's same live statement block. Both the binding and call
   must have an ordinary statement path to the test. Aliases, mutation, rebinding,
   conditional calls, deferred calls and nested closure chains are unknown.
   A spawned thread's closure counts only where its panic reaches the test
   thread (#6966): the plain closure is the only argument of
   `std::thread::spawn(..)` chained directly into `.join().unwrap()` or
   `.join().expect(..)`, or of `s.spawn(..)` where `s` is the sole parameter of
   the nearest enclosing closure, that closure is the only argument of
   `std::thread::scope(..)`, and the spawn is either a bare statement (the
   scope re-raises an unjoined thread's panic) or carries the same join chain.
   The closure `std::thread::scope` runs is itself on an ordinary path: it is
   called once on the test thread and its panic propagates. A `::thread::`
   path names an extern crate, so it never matches. Names are not
   resolved, so the test's file refuses when it holds an item, alias or
   binding named `std` or `thread`; an `extern crate thread;`; a `use` ending in `std` or `thread`
   other than exactly `use std::thread;`, or a `self` in a `use` list under
   a `std` or `thread` prefix; a glob `use` other than `use super::*;`
   inside an inline module (at the top of an out-of-line module file it
   globs a parent in another file);
   `use`, `mod` or `extern` inside macro tokens; `include!`; or an item- or
   statement-position macro other than a std statement macro (`println!`,
   `assert_eq!` and the like). A `thread::` path also needs
   `use std::thread;` directly in the test's own module. Residuals: a bare
   scoped spawn followed by a diverging call in the scope body
   (`std::process::exit`), an
   attribute or derive macro that emits such an import, a cfg'd-off
   `use std::thread;` beside an extern crate renamed `thread`, a `#![no_std]`
   root aliasing `std` in another file, and a `#[macro_use]` macro from
   another file that reuses a std statement-macro name (`assert_eq!`) to emit
   one. Only a whole `use std::thread;` counts as the import; the same path
   nested in a list (`use crate::fake::{std::thread};`) refuses. Detached threads, bound handles,
   and joins whose result is dropped or converted (`.ok()`, `let _ =`) stay
   unknown.
   Constant-row tables (#5328): the body of an unlabeled `for` loop is on
   the path when the loop's iterable is a non-empty array of constant rows,
   so its first iteration runs. The array is written inline (`for r in [..]`,
   `&[..]`) or bound once by a plain immutable, unattributed `let` in the
   statement list that holds the loop, whose name (raw `r#` spelling
   included) has no other token in the test (no shadowing, mutation, alias
   or second use). No attribute may appear anywhere in the rows, since a
   `#[cfg]` can remove every element. Every row leaf is a literal, a negated
   literal, `None`, `Some`, `Ok` or `Err` (bare or called), a qualified path
   whose segments are all CamelCase other than `Self`, without generic arguments (`Kind::Empty`,
   `Status::Complete(5)`), or `vec![..]` whose tokens are literals and the
   punctuation `[ ] ( ) , - &`; parentheses, references, tuples and nested
   arrays of these are constant. A bare CamelCase name may be a `fn` or
   `const` and is refused, as are calls, methods, SCREAMING_CASE consts,
   ranges, indexes and repeat arrays (`[r; n]`), since they may be empty or
   carry the owner's own output. A `break` or `continue` anywhere in the
   loop before the assertion refuses it, as for `loop`. This admits the
   assertion's execution only; every other rule still applies to it.
   RIPR-SPEC-0186 says when a loop-bound argument is a boundary input.
   `?` in a root test remains supported (an error fails an ordinary Result
   test); `?` in a closure is refused because its result could be discarded.
   Exactly
   one compared operand is a complete call of the owner's name with nothing
   chained after it, and the other operand does not mention the owner's
   name (`assert_eq!(f(4), f(2) + f(2))` compares the owner with itself).
   An `unsafe { .. }` block whose only content is that complete call (no
   statement, nothing chained after the call or the block; comments around
   the call are ignored) is the call:
   calling an `unsafe fn` needs the block, and the block's value is the
   call's value.
   A compared operand that is a plain identifier the test binds exactly
   once, by an immutable `let v [: T] = <call>;`, is that call (#6974) when
   `v` appears nowhere else in the test but as a whole operand of
   `assert_eq!` assertions on later lines, and the assertion is the
   statement right after the `let` and the only statement on its line (a
   statement between them, or one sharing the assertion's line, could
   change the value through a shared handle). A `mut` binding, a second binding, a
   borrow, a method call or argument use of `v`, a use before the `let`,
   or an initializer with anything around the call leaves the assertion
   unpinned. An expected operand naming a `let` whose initializer mentions
   the owner, directly or through further `let`s, compares the owner with
   itself and is refused, and so is one naming a value ripr cannot trace
   to a simple `let` (`let (same, _) = (f(4), 0);`). This binding scan runs
   only when the test names the owner outside the assertion under review;
   with no other mention, no binding can hold an owner call, so an
   unrelated `let (want, _) = (12, 0);` keeps the pin. Residual, shared with
   the bare call on main: an item `const` or `static` initialized from a
   `const fn` owner (`const W: u32 = crate::weight(4);`) is not followed.
2. Call identity, from the parser's item-container fact on the owner
   (`FunctionFact.item`: free, local, inherent, trait impl, or trait, with
   the `self`-receiver and body flags; the lexical fallback leaves it
   unknown):
   - A bare `name(..)` names only a module-level function without a `self`
     receiver. No other module-level function of that name may exist in the
     workspace, the test must not bind the name (`let`, nested `fn`, the
     test's parameters, a `for`, closure or match-arm pattern, or a macro
     such as `let_assert!` that mentions it), and the test's file must not
     rename an item to it (`use a::b as name`). As for a path (below), the
     owner must sit directly in a module ripr can place by parsing its
     file, and neither the owner nor any enclosing inline module may carry
     a `cfg` or `cfg_attr` attribute, outer or inner (#7082): a
     complementary cfg may compile a same-named `static`, `const` or `use`
     that the bare name reaches instead. Nor may the owner's file be
     droppable by a cfg: a `cfg`, or a `cfg_attr` whose attributes name
     `cfg`, `cfg_attr` or `path`, as an inner attribute at the top of the
     owner's file or of any file on the chain that compiles it into its
     crate, or on the out-of-line `mod name;` declaration of each step of
     that chain, lets a same-named module replace the whole file. A
     `cfg_attr` that only toggles lints or docs
     (`#![cfg_attr(docsrs, feature(doc_cfg))]`) does not. An include edge,
     an unresolved chain, or a non-root file with no recorded chain (ripr records none for a `#[path]` it cannot resolve,
     such as one under `cfg_attr`) fails closed
     (`a_cfg_gated_owner_is_not_reached_by_a_bare_call`,
     `an_ancestor_file_a_cfg_may_drop_gates_the_owner`,
     `a_cfg_attr_path_on_the_owners_declaration_gates_it_through_real_composition`).
     The same file rule applies to a path call. The test's binding of the
     name must resolve to the owner (#7097): a `static` (plain or `mut`),
     `const`, tuple or unit struct, or extern item of the name in the test's
     body or own module scope takes the call, so it refuses; an explicit `use` binding the
     name must be the only one in its scope and resolve to a module
     holding the owner — the owner's own path (`self`/`super` from the
     scope, `crate` under the owner's root, or the owner's library crate
     name from the manifest authority) or one `use` re-exporting it; a
     glob must deliver the owner's binding from such a module holding no
     twin (an item, a `use` to elsewhere, or a macro that may emit one);
     and with neither `use` nor glob the test must sit in the owner's own
     module. Type-only items (`mod`,
     `trait`, `type`, `enum`, `union`, braced `struct`) share no
     namespace with the call and keep the pin. A `super` at the top of an
     out-of-line file climbs into the inline scope holding its `mod`
     declaration in the declaring parent, so `use super::name` and
     `use super::*` there resolve past it; include edges never climb.
     Anything unplaced — a
     foreign crate, an unresolved path, a re-export chain past one hop —
     refuses
     (`a_bare_call_to_an_imported_same_named_twin_is_not_a_pin`,
     `a_bare_call_beside_a_competing_import_is_not_a_pin`,
     `a_same_named_item_in_the_test_file_is_not_the_owner`,
     `a_bare_call_through_the_owners_own_import_pins_beside_a_twin`,
     `a_bare_call_through_a_glob_pins_only_past_the_owners_module`,
     `a_bare_call_without_an_import_pins_only_in_the_owners_module`,
     `a_same_named_type_only_item_in_the_test_file_keeps_the_pin`,
     `a_unit_struct_twin_refuses_the_bare_call`,
     `an_extern_twin_refuses_the_bare_call`,
     `a_static_mut_twin_refuses_the_bare_call`,
     `an_out_of_line_super_glob_pins_past_the_declaring_parent`,
     `an_out_of_line_super_use_pins_past_the_declaring_parent`,
     `an_out_of_line_super_glob_refuses_past_a_parent_twin`).
   - A path call `a::b::name(..)` (#6974) names the same free function only
     when the path resolves to exactly the module that declares the owner.
     There an explicit `fn name` takes the value name from every glob, and
     any other item of the name (a `use .. as name`, a re-exported variant,
     a tuple struct, a macro-emitted item) fails to compile, so imports and
     types elsewhere cannot capture the path. Every segment is a plain
     identifier not starting with an upper-case letter (no leading `::`, no
     generic arguments, no `r#`), and the path is one of:
     - `self::` or `super::`, followed by any run of `super` and then module
       names, from a test in the owner's own file, resolved from the
       test's inline modules without climbing out of the file;
     - `crate::` and module names, from a test that composes under the
       owner's crate root, when the owner's file is that root;
     - an import name of the owner's library from the test's crate manifest
       (`krate::name(..)` in an integration test), and module names, when
       the owner's file is the library root. No workspace file may rename
       another item to that name or spell it `r#name`, and the test's crate
       may not bind it itself (a `mod`, `struct`, `enum`, `union`, `trait`
       or `type` of the name, a `use` of the name, or any glob, in a file of
       the test's crate).
     The owner must sit directly in a module (not a fn body or an `impl`)
     that ripr can place by parsing its file. Neither the owner nor any
     enclosing inline module may carry a `cfg` or `cfg_attr` attribute,
     outer or inner (`#![cfg(..)]` in the fn body or module): a
     complementary cfg may compile a same-named `static`, `const`, `use` or
     module in its place, so the path no longer has to reach the owner. Any `r#name` of the owner's
     name in the workspace (a raw twin the uniqueness gate's name match
     misses) and any macro invocation in the owner's crate whose input names
     the owner (`twin!(name)` may emit a `fn name`
     under a cfg that drops the owner; the assertion macros listed as
     non-returning are exempt) refuse the path. A path to any other module,
     including one that re-exports the owner, does not pin.
   - A method call `recv.name(..)` names only a function with a `self`
     receiver in an `impl` or `trait` block. `recv` must be a plain local
     binding, and every `let` that binds it must be a simple
     `let [mut] recv [: T] = ..;` whose type ripr can read: an annotation,
     a struct or tuple literal (`T { .. }`, `T(..)`), or a constructor call
     whose signature returns the type: `T::default()` and `T::from(..)`
     directly, `T::try_from(..)` only after `?`, `.unwrap()` or
     `.expect(..)`, and any other `T::f(..)` only when `f` is `T`'s one
     inherent associated function of that name and declares `-> Self` (or
     `-> T`), or `Result`/`Option` of it followed by `?`, `.unwrap()` or
     `.expect(..)`; a unit struct's own name (`let recv = Unit;`); or a
     byte-slice expression (`&[..][..]`, `&b".."[..]`). A receiver that no
     `let` binds is a path expression: `Unit.advance()` is typed as
     `let recv = Unit;` would be (#7083). Telling what a bare name resolves to
     takes name resolution, so a bare name types its receiver only in a shape
     where nothing else can supply the name. The workspace must declare it
     exactly once, as a non-generic unit struct (`struct Unit;`), and in the
     test's own file. Every other spelling of the name in the workspace must
     be an `impl` header, a method call (`Unit.f(..)`) or a `let`
     initializer (`= Unit;`). No workspace `macro_rules!` matcher may take an
     `ident` or `tt` fragment, no file that defines a macro may spell the
     name (one `struct Unit;` in a macro body declares a type per
     invocation), every workspace glob import must be a `crate`,
     `self` or `super` path through declared workspace modules that no
     workspace `use` or `extern crate` also binds (so neither a foreign glob
     nor `use std::u32 as nums;` with `use crate::nums::*` qualifies, even
     beside an unrelated `mod nums`), no workspace file may use
     `include!` or `#[path]` (either spelling, including raw `#[r#path]`),
     and the name may not be a prelude value
     (`None`, `Some`, `Ok`, `Err`). Review found that imports
     (`use self::Kind::Unit`, a lower-case `pub use std::u32::MAX`
     re-export), raw identifiers, macro input, `include!`, Unicode
     whitespace inside a macro matcher and a prelude name can each put
     another value under the name, so any of them refuses. Items a derive
     or attribute proc macro emits stay invisible, as for every other rule,
     as do items an out-of-workspace `macro_rules!` invocation emits (#7160).
     An inline receiver `T::f(..).name(..)` is typed exactly as
     `let recv = T::f(..);` would be, so `Stack::new(1).depth()` pins
     `Stack::depth` under the same constructor-signature rules.
     A name bound by any other pattern (a closure parameter, a `for` or
     match-arm pattern, a destructuring `let`, a nested `fn`, the test's
     parameters, a macro that mentions it) leaves the type unestablished. A
     named type must be a struct, enum or union declared in the workspace,
     and the test's file must not import it from outside the workspace,
     rename another item to it, or declare a `type` alias of it. A rename
     refuses in either spelling: `r#Window` denotes `Window`, so a
     raw-identifier alias rebinds the same name (#7067). Related-test
     reach shares the single-file rename refusal: a rebound receiver keeps
     a name-only relation, never `direct_owner_call`. A type
     declaration of the name in the test's own module scope shadows the
     production type for that test (#6905), so it refuses the pin rather
     than crediting the production method. A macro definition or invocation
     in that scope whose text declares the name may emit the type, so it
     shadows the same way (#6948 review). The owner's own enclosing module
     is not a shadow (#6957): when the production declaration sits in an
     enclosing non-root module, a binding of that name still names the
     production type. A same-file test-module declaration alongside it, and
     any test-file declaration for a cross-file owner, still refuses. A
     declaration of the name at an out-of-line parent module's root
     shadows the same way (#6950): the test file's composed parent chain
     is its enclosing scope beyond its own file, except the parent root
     holding a root-level owner, which is the production scope. A
     root-level `use ... as <name>` rebinds the name to a different type
     and shadows too; a plain root-level `use` may re-export production,
     so only renames refuse. An unresolved chain, an `include!` edge, or
     a missing or unparseable parent refuses rather than guessing.
   - The receiver type must dispatch to the owner: the inherent `impl`'s
     self type, the trait impl's self type, or, for a trait default method,
     a type with an `impl .. Trait for <type>` in the workspace. A trait
     method also needs its trait in scope: the test's file imports it by
     name from a workspace path (`crate`, `self`, `super`, a workspace
     package, or its `::`-rooted form), or declares it. A byte-slice
     receiver never credits a name `&[u8]` itself resolves (slice methods,
     prelude and `std::io` trait methods). A named receiver never credits
     a by-value prelude trait method name (`count`, `map`, `into`,
     `into_future`, `Iterator`'s `eq`/`ne`/`cmp`/`partial_cmp`/`lt`/...):
     method lookup tries `T` before `&T`, so `Iterator::count(self)` takes
     `c.count()` before an inherent `count(&self)` whenever the type is an
     iterator, and ripr cannot see which std traits a type implements.
   - No other workspace definition of the name with a `self` receiver and a
     body may exist (an override, another type's inherent method, another
     trait's method), except a pure `(**self).name(..)` / `(*self).name(..)`
     forward. A definition the parser did not index (a `macro_rules!` body)
     counts when its `fn name` text is visible. A definition in a
     lexical-fallback file always counts.
3. Return path. The changed expression must be the owner body's tail (or
   its final `return <expr>;`), and it must evaluate all of its parts on
   every input: no closure, `|=` or non-binary `|`, no `&&`/`||`, no `if`, `match`,
   loop or `break`, and no combinator that skips its argument on some
   inputs (`map_or`, `unwrap_or`, `and_then`, `then`, ...). With
   `x.map_or(0, |v| v * 3)` changed, `assert_eq!(f(None), 0)` never runs
   the changed closure. A binary bitwise `|` (#6675) evaluates both
   operands on every input, like `&`, `^`, `<<`, `>>`, `+` and `*`, so
   `u16::from(lo) | (u16::from(hi) << 8)` is unconditional. A `|` counts
   as binary only when it directly follows a completed operand token: an
   identifier or number other than `async`, `break`, `else`, `in`, `let`,
   `move`, `mut`, `return`, `static` or `yield`, or a `)`, `]` or `?`. A
   closure's opening pipe never does (`f(|x| ..)`, `move |x| ..`,
   `Foo { f: |x| x }`, `[|x| x]`), so every closure still fails closed. A
   `|` inside a macro invocation's arguments
   (`matches!(k, A | B)`) or in a tail holding a `let` may separate pattern
   alternatives, which short-circuit, so it is never binary. When the owner has no `?` and no other
   `return`, any pinned value came through it. Otherwise the changed
   expression must be one `Ok(..)` (or `Some(..)`) constructor, the only
   one in the body, every other `return` must build `Err(..)` (or `None`),
   and the pinned value must itself be `Ok(..)` (or `Some(..)`). The mirror
   case covers a changed early `return None;` (or `return Err(..);`): it
   pins only when it is the body's one `return` of that value, every other
   `return` and the tail build one `Some(..)` (or `Ok(..)`) call, the body
   has no `?`, no `return` sits in a closure, `async`/`const` block or
   nested `fn`, the changed value evaluates all of its parts (no `if`,
   `match`, `&&`/`||` or skipping combinator, as for the tail), and
   the pinned value is exactly `None` (or an `Err(..)` call). bytesize's
   `as_whole_units` (`return None;` beside a `Some(self.0 / unit)` tail) is
   the motivating shape. An owner
   body that invokes any macro outside a fixed non-returning set
   (`assert!`, `format!`, `panic!`, `vec!`, ...), however it is spaced
   (`ensure !(..)`), leaves the return paths unestablished.
4. Macro binding. The existing non-returning standard-macro vocabulary is
   reused for test bodies; any other invoked macro may hide an early exit
   and refuses the pin. Only bare standard names qualify; qualified paths stay unknown because
   their roots can be rebound.
   Escape keywords or nested/ambiguous invocation syntax in opaque macro operands
   also refuse it; nested expansion is not interpreted. Any visible
   workspace definition/import binding of a trusted macro, a foreign glob,
   `macro_use`, `no_implicit_prelude`, or macro-generation arguments naming a
   trusted macro used by that test refuses the pin. This scan
   includes other indexed files because textual macro scope reaches child
   modules. Workspace-owned globs remain supported only when that scan finds
   no competing binding. The scan conservatively ignores visibility/cfg
   refinements, so unrelated same-name definitions can cause under-credit.
   Other trusted names unused by the test do not defeat its pin (for example,
   an unrelated `std::fs::write` import does not defeat a plain assertion). It is not
   macro expansion or full name resolution.
5. The existing owner-binding defeats still apply: a foreign same-name
   import, a same-named function in the test's own package when the owner
   lives in another package, and the exact variant when the changed
   expression constructs an error variant.
   An import is not foreign when its first segment names the owner's own
   library from the test's crate, as the manifests declare it. The owner
   file must compose (through its `mod` declarations) under its package's
   default library root, `src/lib.rs`, with no `[lib] path` and `autolib`
   not turned off. Then either the test is in the same package (the
   `[lib]` name, else the package name), or the test's nearest manifest has
   a `[dependencies]` or `[dev-dependencies]` entry (also spelled
   `dev_dependencies`; `[target.*]` tables included) whose `path`, directly
   or through `[workspace.dependencies]` for `workspace = true`, resolves to
   the owner's package directory. A `package =` rename must name the
   owner's package and imports under its key; without one the key must be
   the package name and imports under the library name. Target tables are
   read without their `cfg` predicates, so a name that any other entry
   (in any table) binds to another package is ambiguous and stays foreign.
   A `git` or
   `registry` key, a `package.workspace`, a `[patch]` entry for the name or
   any `[replace]` between the test and the root, and a `.cargo/config`
   between either file and the analysis root that mentions the name or sets
   `paths`, `[patch]`, `[source]` or `include` leave the import foreign
   (configuration above the root or in `$CARGO_HOME` is not read). A test
   in the owner's own package whose dependency key is the library name
   also leaves the import foreign, and so does any sign that the library
   may export another item under the callee's name: a `pub use` in a
   library file that names the callee or globs, unless its path is rooted
   at `crate`, `self` or `super` and passes only through modules the
   library declares that no `use` or `extern crate` in it also binds,
   checked for every path inside a brace group
   (`use fastscore as fs; pub use self::fs::score;` and
   `pub use self::{fs::score};` are refused, even beside an unrelated
   `mod fs`); a library `const` or `static` of that name; an `include!` in
   a library file or an unresolved include anywhere; or a file under the
   package's `src/` whose crate root is not established. The same
   own-crate reading serves every consumer of the same-name import defeat
   (the reveal-side owner binding, RIPR-SPEC-0229 arm withholding and tuple
   match observations), as the root package's names already did. So
   `use pricing::score;` in a sibling member that depends on `pricing` by
   path pins `score`, and the same line under a `pricing` key that names
   another package does not.
6. Clone field pins (#6692). A `field_construction` probe on field `f` of
   a hand-written `impl Clone for T` is confirmed by
   `assert_eq!(recv.clone(), recv)` (either operand order). The owner is
   `clone` with a `self` receiver in an `impl Clone for T` block whose
   trait is the standard one (bare `Clone`, `std::clone::Clone` or
   `core::clone::Clone`, the latter two only while no workspace file renames
   an item to `std`/`core`; `impl dupe::Clone for T` is refused, and so is an
   owner file that imports, globs or renames `Clone` from elsewhere) and
   whose self type has no generic arguments (`impl Clone for W<Foo>` is
   refused), and the
   changed line lies in a `T { .. }` or `Self { .. }` literal that is the
   body's whole tail and only exit: no `?`, no `return`, no macro outside
   the non-returning set, and no part evaluated only on some inputs (rule
   3's tail gate). `T` is declared once in the workspace with
   `PartialEq` in a plain `#[derive(..)]` list (not behind `cfg_attr`),
   carries no other attribute that may change equality, declares no generic
   parameter (type, const or lifetime: `struct W<String>` names a parameter
   `String`, and an instantiation-specific `impl` may stand beside the
   derive), has no path-qualified derive entry (`#[derive(foo::PartialEq)]`),
   does not also derive `Clone` (the hand-written clone may be gated beside
   it), and has no hand-written `impl PartialEq<..> for T` in the workspace
   (any `Rhs`). The declaring file may not import, glob or rename
   `PartialEq` from outside `std`/`core`/`alloc` (a shadowed derive macro),
   by the same reading as the field-type names below. `f` carries no
   attribute, and its type compares by value as RIPR-SPEC-0225 rule 4
   defines (a standard value type, a reference, tuple, array or standard
   container of such types, or a workspace type that meets these equality
   rules recursively, each with its own declaring file). Rule 4's standard
   containers are read here as `Option`, `Result`, `Vec`, `VecDeque`, `Box`,
   `Rc`, `Arc` and the `BTreeMap`/`BTreeSet`/`HashMap`/`HashSet`
   collections; a workspace type with generic arguments or parameters fails
   closed. A standard name must denote the standard type: a
   multi-segment type path must be rooted in `std`, `core` or `alloc`
   (`foreign::String` and `crate::String` are refused, and so is any root a
   workspace file renames to, `extern crate other as std;`), and the file
   declaring the field's type may not rename an item to the base name
   (`use x::Thing as String;`), alias it (`type String = ..;`), or name it
   or glob in a `use` rooted elsewhere (`use foreign::Vec;`,
   `use foreign::*;`). A `crate`/`self`/`super` or workspace-package `use` may
   bring only a workspace-declared type of that name, and a workspace glob
   counts only while no workspace file re-exports the name or a glob from
   elsewhere, renames to it or aliases it. The scan is file-wide, not
   per-module. The changed line must be one whole field
   initializer at the literal's own brace depth: a field of a nested literal
   or an argument of a call within a field (`start: f(Raw { start: .. })`)
   is not the outer field's value. No workspace `trait Clone` may exist, the
   test's file may not import a foreign `Clone`, and no other `fn clone`
   with a receiver may compete. The test side reuses rules 1, 2 and 4 with
   two changes: the non-owner operand must be exactly the clone call's
   receiver, and that receiver must be built without the clone under test.
   Every `let` of it initializes it with a `T { .. }` or `T(..)` literal or
   a call to `T`'s one inherent constructor, and every field initializer or
   argument is a trivial expression: literals, `true`/`false`, constants and
   variants named by a path whose last segment starts uppercase
   (`u32::MAX`, `Kind::A`), `as` casts to primitives, and only the operator
   characters `+`, `-`, `/`, `%`, `&`, `^`, `<`, `>` and `=` (so `+`, `-`,
   `/`, `%`, `&`, `&&`, `^`, `<<`, `>>`, comparisons and a `&` reference).
   `*`, `|`, `!`, `?`, parentheses, brackets and braces refuse, and so does a
   call, method, macro, index, block, closure, deref, range, `..base`
   update or local binding
   (`Window::new(make(&base), 9)`, `Window { start: helper(&base), .. }`,
   `Window::new(s, 9)`): any of them may carry a wrong clone's output. The
   constructor's body must be nothing but a `Self`/`T` literal of trivial
   items over its parameters (field shorthand allowed); a body that calls a
   helper (`copy_of(&Window { start, end })`) is refused. No initializer
   mentions `clone`, `clone_from`, `cloned` or `to_owned`
   (`let w: Window = base.clone();` is refused, since an idempotent wrong
   field would survive the comparison); `Default`/`From` constructors are not
   read; and the test may not reassign the receiver, and no `mut` may
   precede its name (`let mut w`, `&mut w`, `ref mut w`): a field write
   (`w.start = w.end`), a `&mut w.start` borrow or a `&mut self` method call
   (`w.set_start(9)`) all need a `let mut` binding, so one alone refuses.
   Anything ripr cannot read fails closed: a lexical-fallback owner file, a
   duplicate declaration of `T`, generic arguments on a workspace field
   type, a UFCS `Clone::clone(&w)` call, or a receiver bound some other way.
   `assert_ne!`, a comparison with any other value
   (`assert_eq!(w.clone(), Window::new(3, 9))`) and a hand-written
   `PartialEq` give no credit. A confirmed clone pin clears the
   `FieldValue` missing discriminator for that probe only when reveal
   credits it: the pinning assertion matched in a test that may supply the
   oracle (not a name-only relation next to a reach-bearing test) and the
   pin remained after the foreign same-name import and cross-package same-name
   defeats (`RevealOutcome::owner_pin_credited`). The finding may then
   read `exposed`; no other family or oracle gains credit (the
   #6579 whole-object effect-observer gap is unchanged).
7. Pins equal to an operand (#7077). An exact pin on a constructed field
   cannot notice the field replaced by one of its operands when, for the
   pinned input, the field equals that operand. The changed initializer is
   `f: a <op> b` with two distinct plain identifier operands and one binary
   operator, and both operands are established primitive, so the dropped
   operator is a built-in one: a parameter of a primitive type, a `let`
   annotated with one, a numeric or bool literal, an arithmetic
   combination of established names and literals with the accepted operators
   and `as` casts to primitives, or a call whose every same-named
   function in the index declares one bare primitive return (optionally
   behind one reference). Anything else — a string, char literal or block
   comment anywhere in the owner body, a callee missing from or ambiguous
   in the index, a non-primitive return, a binding that is not established — keeps the
   credit: a custom type can overload the operator, and an overloaded
   operator may carry side effects that assertions on other fields
   observe (review of #7084). A `let` that rebinds a primitive-typed
   parameter shadows it: the name is not established up front, and the
   rebind must prove primitive itself (its own initializer may still
   refer to the parameter's value before the rebind). A sibling initializer of the same struct
   literal is `g: a` (or
   the shorthand `a`). A related test binds `q` once, straight from a call
   to the owner (`let q = quote(..)`; a second `let q` may shadow it and
   refuses), and holds both `assert_eq!(q.f, v)` and `assert_eq!(q.g, v)`
   with the same literal `v` (an integer with digit separators ignored, a
   string, a char or a bool; a name or call may differ between the two
   pins and refuses). For that input `f` equals `a`, so the mutant `f: a`
   passes those pins. When every exact pin on `f` in every
   related test is paired this way with the same sibling, the
   field-construction finding's discriminate stage reads weak with the code
   `field_pinned_equal_to_operand`, which names the operand the tests never
   vary, and that summary is the finding's missing evidence. Other mutants
   of the initializer (`a * b` to `a + b` with `b == 1`) may still be
   caught; the downgrade claims only the dropped operand. Any other mention
   of `f` in a related test (a pin with a custom message, another assertion
   macro, a value read out of the result, a second receiver pinning another
   value) keeps the credit. So does a related test that reaches the owner (it
   names the owner, or is related by a direct or helper owner call) without
   naming `f` (whole-struct equality, a snapshot, a helper that asserts), and a
   pinning test with a condition, match, loop, closure, early exit or `?`,
   where a pin may not run. A pinning test must hold only receiver bindings
   that are a bare owner call (no chained transform such as
   `.with_coupon(..)`) and assertions that call nothing and read receivers
   through plain fields: a helper inside an assertion, a
   whole-result check (`assert_eq!(q, expected)`, a method call, a helper
   such as `check_quote(&q)`), a second owner result, an attribute such as
   `#[cfg(..)]`, or the owner named in an assertion keeps the credit.
   Anything else ripr cannot read (a nested expression, a repeated
   initializer text) keeps it too.

7. Borrowed check-helper assertions (#6482). A test may borrow the
   `assert_eq!` of a test-local check helper it calls, so a table of pins
   written as `check_tip(40, 6, 46); check_tip(10, 0, 10);` over
   `fn check_tip(b: u64, t: u64, want: u64) { assert_eq!(with_tip(b, t), want); }`
   is admitted the same way as the assertion written inline. Rule 1 is applied
   across exactly one call, and every condition fails closed:
   - The test itself passes every rule-1 context gate (item ancestry, cfg,
     escape and macro gates, unique identity).
   - The test calls the helper as a bare single-segment path with no generic
     arguments (`check_tip(..)`, never `self::check_tip(..)`,
     `check_tip::<T>(..)`, a method, or text inside a macro operand), and that
     call is on the test's eager path under the same rule-1 walk as an inline
     assertion: not inside a loop, branch, argument, deferred closure or
     `async` block, after no root `return`, with no attribute. A `for` loop
     over a non-empty constant-row table (#5328) is an eager path here
     exactly as for inline assertions. A call inside any closure, even a
     directly invoked one, is refused: the helper-assertion producer never
     credits it to the test. One eager
     call is enough; further deferred calls neither add nor remove credit.
   - The call can only name the helper: exactly one `fn` of that name is
     visible anywhere in the file, and it is a direct item of the test's own
     module (or both are top-level items). The helper is either a
     `CfgTestModule` function or a top-level non-test `Production` function
     in a crate-root integration-test target (`tests/<name>.rs` or
     `tests/<name>/main.rs` relative to the nearest owning manifest,
     including `crates/*/tests/…` and a package nested under `tests/`
     such as `tests/harness/tests/…`, #7125). A nested package (nearest
     manifest is not the workspace root) is credited only when Cargo's
     metadata inventory lists that autotest as a libtest-enabled workspace
     test target, so `[workspace] exclude` and `harness = false` cannot
     become test evidence. A declared `[[test]]` with `test = false` is
     skipped by `cargo test` and is not credited. The
     producer does not reclassify that `Production` helper; it only copies
     the helper's calls and parser-backed assertions onto the calling
     test. A `Production` function in a production file, including a
     `src/tests/` module directory, a nested `tests/support/` file (including
     `tests/support/tests/`), and a
     helper in `benches/` or `examples/` (including `examples/tests/`),
     stay uncredited. An undeclared `tests/<name>.rs` file in a package
     that sets `autotests = false` is not a Cargo target; diff analysis
     drops it before helper crediting (#6965). Root-package autotest
     roots stay path-shape except an established `harness = false` or
     `test = false` target; nested-package membership uses the existing
     Cargo metadata authority and fails closed when the probe is
     unavailable. A module item
     cannot coexist with a
     same-named import and wins over a glob. The test contains no `use` item
     and binds no name equal to the helper (pattern, parameter, closure
     parameter or nested item). A helper in a parent or sibling module, a
     helper only a macro generates, and a duplicate definition in another
     module of the file are refused.
   - The helper runs to its end on every call: no attributes other than
     `#[track_caller]`, no generic parameters or `where` clause, not `async`,
     `const` or `unsafe`, no `self` parameter, no return type, and no `return`
     or `?` anywhere in its body. It passes the same escape gate as a test
     (only the trusted standard macros, no `break`, `continue` or `yield`) and
     the same item-context and cfg gates, and the macros it invokes join the
     workspace macro-binding check of rule 4. A scoped binding (a
     `macro_rules!` or `use` inside the helper's own body) is checked at the
     borrowed assertion's line as well as across the test's span, since the
     helper's scope never overlaps the test's.
   - The assertion sits on the helper's own eager path and is uniquely
     identified by its line and text, as rule 1 requires within a test.
   - The assertion must also reach the test's oracle facts through the
     index's same-file helper crediting (`facts/test_helpers.rs`). Both
     authorities must agree; the admission adds no assertion to a test.

   For the owner-return pin, a borrowed assertion qualifies only through a
   bare call. A method call is refused, because the receiver's type would be
   read from the test's bindings, which do not bind the helper's parameters.
   The rule-2 bare-call binding defeats apply to the helper's text too
   (parameters, `let`, `for`, closure and match patterns, nested `fn`, macro
   operands), and the calling test must not mention the owner's name at all,
   so no call-site argument can feed the owner's own value back as the
   expected one (`check_tip(40, 6, with_tip(40, 6))`). The helper names the
   owner exactly once, in its asserted call: the test-body self-comparison
   scan never reads the helper, so `let e = with_tip(b, t);
   assert_eq!(with_tip(b, t), e);`, an alias `let w = with_tip;` or a second
   `super::with_tip(..)` refuses the loan. Rule 3 is unchanged:
   an owner with other exits still needs an `Ok(..)`/`Some(..)` expected
   value, which a helper parameter is not, so such a pin stays refused.

   Decision rationale. Refusing every out-of-body assertion was correct while
   nothing established that the helper ran. The narrowest sound extension
   reuses the existing execution walk twice (the test's call, the helper's
   assertion) and adds a resolution gate that rests on two Rust rules a parser
   can check without name resolution: a module item conflicts with any
   same-named import in that module, and a function body with no exits, loops
   or untrusted macros runs to its end. Following helpers further (helpers of
   helpers, cross-module or cross-file helpers, generic helpers, helpers with
   early exits, calls in a loop other than a constant-row table) needs value, dispatch or loop-count
   reasoning this bounded query does not do, so those shapes stay refused
   rather than approximated. The decision stays in the existing owner-pin
   owner (`syntax/owner_pin.rs` for context, `classify/owner_pin.rs` for the
   pin), so every family that consumes the shared admission (`return_value`,
   `error_path`, `predicate`) sees one answer.

### Bool-owner pins

An owner whose signature declares `-> bool` has two values, so a bare
`assert!(owner(..))` pins its whole return value to `true` and
`assert!(!owner(..))` to `false`, exactly as `assert_eq!(owner(..), true)`
would. Such an assertion is a pin when:

1. The owner declares `-> bool` (the signature text, not inference).
2. The assertion is one plain, unqualified `assert!` (never
   `debug_assert!` or a path-qualified macro) whose condition, the first
   argument, is a complete call of the owner with nothing chained after it,
   optionally negated by one `!`. A conjunction, a comparison, a chained
   call or a double negation pins nothing about the owner's result. Message
   arguments do not decide whether the test fails and are not read.
3. Every other rule above holds unchanged: execution and macro admission
   (the parser collects `assert!` invocations alongside `assert_eq!`), call
   identity and binding defeats, `#[should_panic]`, and the return-path
   gate. A bool owner with an early `return` has no `Ok`/`Some` head, so the
   gate refuses it.

The pin observes two probe families:

- `return_value`: the changed tail, as for `assert_eq!`.
- `predicate`: only when the predicate is the bool owner's whole tail (the
  return-path gate matches it), so the predicate's value is the owner's
  return value. A predicate inside a branch, or one operand of `&&`, never
  pins.

The assertion's `oracle_kind` stays the classifier's `relational_check`
(RIPR-SPEC-0231 keeps `classify_assertion` the single kind authority). Only
its strength relative to the probe rises to strong, the same probe-relative
adjustment `probe_relative_oracle_strength` already makes per family.
Predicate boundary pairing reads the same pin decision, with the same
foreign-import and cross-package defeats reveal applies, so `exposed` still
needs one test that feeds the boundary input and pins that call's result.
`assert!(gate(50))` alone stays `weakly_exposed` for a change at 10.

Pairing reads only the asserted operands. A boundary call or a binding of
one that appears in a message argument (`assert!(gate(50), "{got}")`) is
formatted, not checked, and never pairs. The line-level activation
fallback applies only when the assertion's operands hold the line's sole
owner call, so `let got = gate(10); assert!(gate(50));` on one line does
not pair either. An owner call or binding spelled only inside a comment or
string literal is not a call or a reference. These rules hold for
`assert_eq!` as well.

### Lone-equality pins

`assert!(owner(..) == v)` fails exactly when `assert_eq!(owner(..), v)`
does, and a terminal Err-return guard `if owner(..) != v { return Err(..) }`
in a test returning `Result` is its assertion twin `assert!(owner(..) == v)`
(RIPR-SPEC-0154). The pin reads the two operands of such an equality as it
reads `assert_eq!` operands when:

1. The assertion is one plain, unqualified `assert!`, or a terminal
   Err-return guard whose twin RIPR-SPEC-0154 establishes.
2. The condition holds exactly one top-level `==`: outside parentheses,
   brackets, braces, comments and strings. A top-level `!=`, `<`, `>`,
   `<=`, `>=`, `&&`, `||`, `=`, closure pipe or second `==` refuses, as
   does a negated condition (`!(a == b)`, and a guard `if !a == b`, which
   is `(!a) == b`). An `==` guard's twin is an inequality and pins nothing.
3. Every other rule above holds unchanged: one side is the owner call shape
   and the expected side does not name the owner, the self-computed
   expected check (RIPR-SPEC-0035) reads the same operands, execution and
   macro admission (the parser keys each terminal Err-return guard by its
   `if` line and whitespace-free condition, with the same eager-path
   gates), `#[should_panic]`, the test's line range and the return-path
   gate. The #6974 let-bound result rule applies to `assert_eq!` only.
4. A guard reads only its condition. A macro inside its body runs only
   when the guard fires, so it is never the pinned assertion, and the
   self-computed check selects its reader the same way: an `assert_eq!`
   spelled in a message, comment or guard body does not switch it.
5. The guard's `return Err(..)` must be the prelude variant, not a shadow:
   a test file that binds the value name `Err` anywhere (a `fn`, `const`,
   `static` or struct constructor of the name, a pattern binding or
   parameter, an import of the name or into the name, or a glob import the
   file cannot see through) refuses every guard twin in the file. A glob
   rooted at `super` inside an inline module or at `self` re-imports only
   the file-local definitions the whole-file scan already refuses, so the
   idiomatic `use super::*;` test module neither shadows nor withholds;
   every other glob (`crate::..::*`, an external crate, a path that may
   leave the file) stays opaque and refuses. A shadowed `Err` can return
   `Ok` on the changed behavior, so the guard passes exactly when it must
   not (`fixtures/owner_return_pin_err_guard_shadowed_err`). The refusal
   is file-wide and spells the whole name: coarser, and conservative — it
   withholds credit, never mis-credits.

Refused, conservatively: a guard with an `else` branch; `if !(a == b)`;
a parenthesised `(a == b)`; a condition split across lines; a guard
inside a closure, nested `fn` or async block, whose `return` leaves only
that body; a second guard or assertion after a guard's `return`; and a
guard in a file that binds the value name `Err`.

## Required Evidence

- Rule 7 (#6482): in the RIPR-SPEC-0219 verdict corpus, `grid-arith-helper`
  and `grid-returns-helper` move from a false actionable gap
  (`weakly_exposed`) to `credited` (`exposed`). With RIPR-SPEC-0186's
  borrowed-assertion pairing, `grid-boundary-helper`, `grid-early-helper` and
  `grid-bool-helper` move the same way. No case gains a false `exposed`
  verdict and no other row changes. The other helper cells stay below
  `exposed` for the causes their inline `-exact` twins share: an `&&` tail
  owner pin (`grid-equality-helper`), a changed arm pattern that selection
  never credits (`grid-match-helper`), and the iterator-closure and `?`
  operand shapes (`grid-iter-helper`, `grid-try-helper`).
- The bytes 7930d93 replay moves both `return_value` findings
  (`try_get_int`, `try_get_int_le`) from `weakly_exposed` to `exposed`, and
  the commit's `Err(TryGetError { .. })` assertion is not the confirming
  oracle.
- A fixture pins the bytes shape (`fixtures/owner_return_pin_trait_method`).
- A fixture pins each trap as non-exposed
  (`fixtures/owner_return_pin_identity_traps`): the associated-versus-free
  bare call, the overridden trait default, the early-exit input, and the
  test-local binding of the owner's name.
- A fixture pins a test-module same-name shadow as non-exposed
  (`fixtures/owner_return_pin_test_module_shadow`, #6905): the test's own
  `Window` with a derived `Clone` runs instead of the changed owner, so the
  clone field stays `weakly_exposed` with its struct-field gap.
- A fixture pins an out-of-line parent-module same-name shadow as
  non-exposed (`fixtures/owner_return_pin_out_of_line_test_module_shadow`,
  #6950): the nested child test binds the parent module's own `Window`,
  so the pin is refused and the relation stays name-only
  (`weak_token_substring`).
- A fixture pins a single-file raw-identifier rename as non-exposed
  (`fixtures/owner_return_pin_single_file_raw_rename_shadow`, #7067):
  the test binds the renamed item through the plain spelling, so the
  pin is refused and the relation stays name-only
  (`weak_token_substring`).
- Unit tests pin every gate with a positive and a discriminating negative.
- Twenty matched fixtures keep effective and ineffective tests separate:
  - `owner_return_pin_direct`, `_called_closure`, `_token_direct`, and
    `_token_called_closure` retain `exposed`; correct libraries pass one test,
    and deliberately wrong `input * 2` libraries fail with actual 8 versus
    expected 12.
  - `_uncalled_closure`, `_shadowed_assertion`, `_expired_closure_binding`,
    `_deferred_closure_binding`, `_macro_operand_return`, `_macro_return`,
    `_if_false`, `_unpolled_async`, and `_token_overlap` pass even the wrong
    library. They retain related-test provenance without an oracle and read
    `reachable_unrevealed`, Observe `no`, Discriminate `no`, strength `none`.
  - `_unknown_singleton`, `_nested_test`, `_cfg_false_module`, `_cfg_false_file`,
    `_cfg_attr_module`, and `_out_of_line_cfg` pin review findings from #5020:
    admission cannot manufacture a singleton match or use an uncollected test.
    Each keeps one collected runtime test and passes either library, while
    the static finding is No/No/None `reachable_unrevealed`.
  - `_no_assertion` is the removal control and keeps that same class/stage
    outcome without the context-refusal diagnostic.
- `cargo test -p ripr --test owner_pin_execution` compiles both library
  variants and each actual fixture test before comparing runtime outcomes.
  All twenty cells are in the RIPR-SPEC-0108 honesty corpus. Additional
  public-API mixed-oracle tests reject confirmation/strength borrowed from
  refused assertions and preserve valid direct evidence in either order.
- The initial narrow repair left refused assertions `weakly_exposed` with
  Observe `yes`, confidence 0.92 and advice to replace broad assertions. The
  `_token_overlap` control additionally remained falsely `exposed` through
  generic token matching. This is why admission precedes all oracle credit.
  The corrected negative cells use the unchanged confidence algorithm:
  three `yes` stages plus two `no` stages and the existing unrevealed-class
  adjustment produce 0.79: `3 * 0.20 + 2 * 0.02 + 0.15`. Medium/High stage
  confidence leaves the RIPR-SPEC-0109 ceiling at 1.0. This is an advisory
  confidence signal for the static result, not a
  probability, protection percentage or runtime-adequacy claim. Reach,
  infection and propagation keep their existing static authorities.
- Human/JSON/agent findings disclose `rust_assertion_context_unestablished`
  and ask to establish execution and macro binding, rather than prescribe
  another equality assertion. No runtime outcome is imported into the
  production classifier. Agent context has no top-level scalar confidence
  field; its gap witness carries `confidence.value: 0.79` with
  `confidence.basis: static_only`, alongside the same No/No stages,
  zero-oracle relation and guidance. Exposed controls have no gap witness.
- Each refusal is disclosed with the first gate that failed: an `assertion
  not credited:` evidence entry and a human `Not credited:` line name the
  test, the assertion's file and line, and the blocker (a `for`/`while`/`if`
  construct, a test attribute such as `#[cfg(..)]`, an opaque `name!` call,
  an `async` test, or the file and line of the macro binding). A refused
  context does not also claim that no assertion or oracle was detected. The
  disclosure is computed after admission and never changes what is credited.
  A refused assertion whose text calls the changed owner is named first.
  Only that one earns the next step that calls the refusal a possible static
  limit to confirm with a real mutation run; a refused assertion that does
  not call the owner (an `Ok`-arm `assert_eq!` beside an error-path change,
  or `if flag { assert_eq!(1, 1) }`) could not observe the change even if
  credited, so the next step stays the generic one.
- Macro-binding ambiguity is scoped to what can bind the name. Any
  mention of a trusted name in another macro's arguments stays ambiguous,
  a plain `assert_eq!(..)` included: to that macro it is only tokens, and
  `define!(assert_eq!(mod tests;))` can emit `macro_rules! assert_eq`
  together with the module whose tests then compile against it.
  `macro_use` or `no_implicit_prelude` anywhere in a non-trusted macro's
  arguments makes every trusted name ambiguous (stricter than the same
  attribute written on an item, which a resolved module can admit). A
  `macro_rules!` confined to an inline module or function body (no
  `#[macro_use]` on any enclosing module, no out-of-line child module)
  refuses only tests inside that item; a glob import from a workspace member
  crate with indexed files is workspace-owned.
- A crate-local binding reaches only tests compiled in the same crate. A
  site is crate-local when it cannot leave the crate whose module tree holds
  its file: a private `use` or glob, `#[macro_use] extern crate`,
  `#![no_implicit_prelude]`, or a `macro_rules!` without `#[macro_export]`
  (a `macro` 2.0 item without visibility). Its crate is the root reached
  through resolved module edges, recognized only when that root is a Cargo
  autodiscovered target (`src/lib.rs`, `src/main.rs`, `src/bin/*.rs`, and
  `tests/*.rs`, `benches/*.rs`, `examples/*.rs`, `build.rs` beside an
  indexed `src/`). humantime's `benches/datetime_format.rs`
  (`#[macro_use] extern crate bencher;`) no longer refuses the library's
  `tests/*.rs` assertions. An unresolved `#[macro_use] mod` may
  `#[macro_export]` its macros, so it stays workspace-wide like exported
  definitions. Exported definitions, `pub` imports, sites in
  another macro's arguments, unparsed files, and any file or test whose
  root is not recognized stay workspace-wide. So does any file another
  crate can also compile: an `include!` fragment or a module below one (a
  recorded include target, include edge, or include target as a module
  parent), a non-root file under a `tests/*.rs` root (a shared
  `tests/common/mod.rs` composes under its first owner only), and, while any
  `include!` in the workspace is unresolved (ambiguous, cfg-conflicting,
  capped, dynamic or unindexed), every file: an unresolved fragment and its
  module children otherwise look like a crate root of their own. The same
  holds while any `#[path]` is unresolvable (`cfg_attr`, non-literal), which
  records no module edge for its target. A withheld file in the
  dependent scope is routed by root only when its own path is a `src/lib.rs`,
  `src/main.rs` or `src/bin/*.rs` root, so named mode matches the full
  closure; every other withheld site stays workspace-wide. Limits: a
  `[lib]`/`[[bin]]`/`[[test]]` `path` that moves a target is not read, so a
  file at a default target path that some other target includes through
  `#[path]` is still judged by its default root.
- `use pretty_assertions::assert_eq;` (or `assert_ne`) under its own name
  counts as the standard assertion only when the importing file's nearest
  `Cargo.toml` inside the analysis root declares `pretty_assertions` as a
  plain registry requirement (version, features, `optional`; through
  `[workspace.dependencies]` for `workspace = true`), the manifest names no
  `package.workspace`, no manifest between the file and the root has a
  `[patch]` entry for it (by key or `package =`) or any `[replace]`, and no
  `.cargo/config` there mentions it or sets `paths`, `[patch]`, `[source]`
  or `include`. A `package`, `path`, `git` or `registry` key binds the name
  to another package that Rust source cannot reveal, so the import then
  refuses with that reason. Limits: Cargo configuration outside the root
  (or found from another working directory) is not read, and a file
  compiled by a package other than its nearest manifest (a target `path`
  or `#[path]` from a sibling) is judged by the nearest manifest.

### Matched before/after observations

The initial fourteen fixture inputs were replayed on retained analyzer `e729ca15`,
the rejected narrow owner-pin-only candidate, and the shared-admission candidate.
Every run produced one complete `return_value` finding. Each exact test also ran
against the correct and deliberately wrong library; every correct-library test
passed. Runtime outcomes remain independent of the static verdict.

| Cases | Count | Old analyzer | Narrow pin-only candidate | Shared admission | Wrong-library runtime |
|---|---:|---|---|---|---|
| Direct and called closure, with/without token overlap | 4 | exposed, 1.00 | exposed, 1.00 | exposed, 1.00 | one failure per case |
| Deferred/conditional/binding/macro negative controls without overlap | 8 | exposed, 1.00 | weakly_exposed, 0.92 | reachable_unrevealed, 0.79 | one pass per case |
| Deferred closure with named-input token overlap | 1 | exposed, 1.00 | exposed, 1.00 | reachable_unrevealed, 0.79 | one pass |
| No-assertion removal control | 1 | reachable_unrevealed, 0.79 | reachable_unrevealed, 0.79 | reachable_unrevealed, 0.79 | one pass |

Six later review controls extend the matrix to twenty cases (forty compiled
library/test subjects). Against the first published #5020 head, the real
Unknown helper singleton reads `weakly_exposed` at 0.92, while the five
non-collectable-test shapes read `exposed` at 1.00. With the repaired admission,
all six read No/No/None `reachable_unrevealed` at 0.79. The singleton heuristic
uses the original test assertion count: removing credit must never create a
new matching signal for an unrelated survivor. The bot's initial `assert!(ready)`
example is a `RelationalCheck` and already family-matches; the separate-line
`assert_ready(true)` helper is the discriminating Unknown control.

The initial return-only slice changed five older guarded-result fixtures: conditional
bare equality rows lose standalone return-value oracle credit. The dedicated
`guarded_result_match` authority remains intact, including its positive control.
Four fixture class outcomes are unchanged; `guarded_result_match_swallowed`'s
return-value probe became unrevealed while its error-path probe initially stayed
weak. The #5027 extension now refuses the same conditional bare equalities as
standalone ErrorPath evidence, so both selected families are unrevealed.
Separate family-filtered honesty cases pin exactly one return-value finding
and exactly one error-path finding, preserving both distinct class contracts.
The original-cardinality repair also prevents an existing lexical
`let result = expect_response(..)` mock-expectation row from gaining a new
singleton match in `guarded_result_match_fail_closed`; other retained evidence
keeps its stages and class unchanged. This is bounded admission, not a claim
to resolve match-arm execution or every other oracle family's provenance.

### Shared error and predicate equality admission

The private execution/collectability/macro-binding witness also gates bare
`assert_eq!` evidence for `error_path` and `predicate` probes (#5027). This is
independent of the owner-return pin: error operands still use RIPR-SPEC-0106,
and predicate activation/boundary pairing retain their own semantic authorities.
Neither a valid error operand nor a boundary-valued call proves that an assertion
executes. Admission precedes matching, observation and oracle-strength selection
for all three named families; refusal preserves the original assertion count so
an unrelated Unknown helper cannot gain singleton credit.

An independent ordinary equality also remains eligible after a uniquely bound,
local empty macro. The syntax witness recognizes only one unannotated
`macro_rules!` declaration with the sole catch-all `($($name:tt)*) => {}` rule,
declared before the invocation in the same or an enclosing lexical module.
The existing workspace binding authority rejects competing definitions,
imports and opaque binding-producing calls. This is not a property-macro name
allowlist or a macro evaluator: other matchers, nonempty/returning expansions,
qualified, imported, shadowed and ambiguous bindings remain unsupported.
The same parser-backed local resolver also removes these empty invocations
from the shared call-fact view. Both call discovery and retained mixed-line
text exclude their arguments; a discarded 100/100 call cannot lend boundary
activation to a real far equality on the same line. Original function/file
source and AST coordinates remain intact. The far equality retains strong
oracle strength and Observe/Discriminate=yes, with weak infection and
`weakly_exposed`, just as when the empty invocation is removed. The real
boundary positive remains exposed and rejects the wrong implementation.
This does not govern arbitrary non-equality raw-oracle consumers or resolve
general macro expansion. The same closure, CFG, collection and statement-prefix
requirements still govern the independent equality. Calls after an assertion
receive no new prefix exception.

The repair changes extracted call facts, so predecessor facts are not
interchangeable. Persisted file-fact/classified keys retain their existing
build identity: an e290 predecessor uses a distinct producer from the repaired
commit even when generations remain file facts 1.18/full 1.29/compact and
sharded 0.35. Favorable predecessor self-hit, repaired-producer refusal and
current reuse are separate controls; no cross-build cache bypass is inferred.

Fourteen family fixtures pair direct and invoked-closure positives with uncalled,
false-branch, macro-shadowed and no-assertion controls. Each family also has an
unconditional owner call before an uncalled assertion: real reach alone cannot
create an observer. Public-API tests compile the correct and deliberately wrong
version of each fixture and require exactly one executed test per subject.
The four effective tests fail against the wrong implementation; the ten
ineffective/removal tests pass it. These independent runtime outcomes calibrate
the fixtures only and are not imported into production classification.

The eight ineffective assertion cells and two removal cells require
`reachable_unrevealed`, Observe `no`, Discriminate `no` and oracle strength `none`.
Four positive cells retain `exposed` and strong exact-value evidence. Mixed
strong/weak/refused assertions are checked in both orders; a refused exact
assertion cannot lend strength to an admitted weak oracle or turn an Unknown
helper into a singleton. Ten related tests exercise the eight-row JSON projection
cap without crowding out admitted evidence. All cells are independently guarded
by family-selected RIPR-SPEC-0108 corpus assertions.

A separate ninety-subject compiled matrix covers all three admitted families:
a direct assertion before/after a root return, a bound closure invoked before/after
a root return, a closure that returns before its assertion, and unrelated ordinary
or CFG-disabled nested helpers before direct/invoked assertions, and unrelated
`async`/`async move` return contexts. Effective direct
and invoked positives are admitted; prior outer returns and closure escapes are
refused. The statement-prefix query retains
all macro, CFG, collection and closure-escape gates; it does not evaluate arbitrary
branch conditions. The existing Result harness retains its earlier strong equality
and its class/confidence; a possible return still refuses a later equality.
Only returns owned by the collected function enter its statement-prefix boundary.
A return in an unrelated async block belongs to that future, even before it is
polled; it cannot hide a later executed outer assertion. Assertions inside an
unpolled future remain refused, and an actual outer return still defeats a later
assertion. This is return-scope information, not an async-harness execution claim.
An ordinary or CFG-disabled nested helper's return cannot escape the outer test;
direct and invoked-closure positives remain admitted with either helper present.
This does not admit an assertion inside an uncalled nested helper, and the earlier
closure-return refusal remains unchanged.
The ReturnValue helper controls use the existing token-direct assertion shape;
the separate owner-return pin still conservatively refuses a test body containing
multiple function declarations. Execution admission does not broaden that binding
proof or turn it into an exact owner-return pin.

The full scan also exposes a deliberate usefulness tradeoff: the existing real
`#[tokio::test]` fixture catches an inverted predicate at runtime, but the bounded
query cannot establish its macro binding and async polling path. It retains owner
and test discovery and `propagation_unknown`, while oracle strength becomes None
and advisory confidence moves 0.66 to 0.43. Its 100/50 input still cannot distinguish
`>` from `>=`; that activation limitation is separate. #5040 owns producer-backed
async-harness execution provenance and the real positive/removal controls. No
Tokio-name exception or claim that the actual test is ineffective is made here.

The same existing human/JSON/context projections explain a refused invocation
and keep confidence advisory and `static_only`. This change does not assert
that all oracle families, assertion macros or arbitrary Rust control flow have
execution provenance.

### Admission consumers and cross-test pairing

Predicate boundary pairing consumes the exact same admission callback as reveal.
A false-branch, uncalled-closure or unpolled-future boundary equality cannot regain
credit after reveal refuses it by borrowing a strong far-input oracle. Matched
same-test and separate-test layouts retain that far oracle's strong evidence and
Observe=yes, while Discriminate stays weak with `same_test_pairing_missing`.
Direct and invoked boundary positives remain exposed. Twelve paired layouts
compile correct/wrong implementations and execute exactly one or two tests per
program, rather than treating a zero-subject or compile failure as a control.
The summary describes an absent admitted boundary-call discriminator without
falsely claiming that every failure involves different tests.

The covered route is Rust diff classification. Its consumer inventory is:

| Consumer | Authority and boundary |
| --- | --- |
| `OwnerPinSyntax` / `OwnerReturnPin` | One parser-backed admission decision; owner-return binding remains a separate narrower proof. |
| `classify/reveal.rs` | Filters before kind, strength, token matching, observation and owner-pin credit. Original assertion cardinality remains separate and unchanged. |
| `classify/boundary_pairing.rs` | Uses the same callback before strength or boundary-subject pairing; raw refused assertions cannot restore exposure. |
| `classifier/evidence.rs` / `classify/decision.rs` | Compose admitted stages; the tuple-specific witness is MatchArm-only and outside these covered families. |
| `classifier/finding.rs` | Strong sink guidance starts from the already-filtered `evidence.related_tests`, not raw assertions. |
| `classify/activation.rs` / `classify/related_tests.rs` | Raw source value/missing-fact and relationship scans remain their own static authorities. They are not oracle admission or runtime execution proof and cannot bypass the final admitted pairing requirement. |
| `test_grip_evidence.rs` | Repository grip has a separate raw-oracle consumer. This diff-path repair does not claim repository-grip execution parity; #4793 owns the shared-witness migration. |

The `observed_values` field retains lexical and bounded statically derived
source facts independently of oracle admission, including enum tokens in
refused assertions. Its historical name and structured fields remain stable;
it does not establish execution or observation. Human and JSON evidence paths
share neutral `source ... value` wording, including source-value cap disclosure.
Direct/invoked error oracles retain strong observation, while uncalled, disabled
or shadowed assertions retain source facts without contradicting Observe=no.
This presentation repair changes no class, stage, score or admission decision.

The two executed bypasses, family dispatch before reveal and raw boundary pairing
after reveal, motivate an admitted-oracle iteration contract under #4793. That
follow-up must carry original cardinality separately and preserve source facts
for diagnosis; it must not introduce another execution checker or globally erase
assertions. This repair shares the existing callback without that larger migration.

## Non-Goals

- Rule 7 follows one hop into one same-module check helper. Helpers of
  helpers, helpers called in a closure or in a loop other than a non-empty
  constant-row table, cross-module and cross-file helpers, generic helpers
  and helpers with early exits keep the rule-1 refusal. A helper's arguments are its parameters, whose values stay
  unresolved (RIPR-SPEC-0229); rule 7 establishes execution and call identity
  only.

- Shared admission covers Rust `return_value`, `error_path` and `predicate`
  evidence from bare `assert_eq!` invocations, a bool owner's `assert!` and
  the lone-equality forms above. Qualified assertion macros, other
  oracle kinds/families and general control-flow or macro resolution retain
  their existing authorities; this is not a general execution-proof system.

- Name resolution or type inference. The receiver typing reads a binding's
  syntax only; a receiver returned by an arbitrary function call stays
  unestablished.
- Methods generated by derive or attribute macros, and `macro_rules!`
  bodies that build the name from fragments (`paste!`), are invisible; they
  are documented residuals.
- A trait from outside the workspace that is also in scope and also names
  the method is not detected.
- Type- and trait-qualified calls (`Type::name(..)`,
  `Trait::name(&mut recv, ..)`) do not pin yet; module paths do (#6974).
- A path call relies on the name's workspace uniqueness. Items that a
  procedural or foreign macro generates are invisible to it, as they are
  to the bare call.
- A helper function the changed tail calls may itself ignore an argument
  on some inputs; the tail gate reads the tail's own syntax only.
- Opaque macro expansion can synthesize a binding from fragments without a
  visible `assert_eq` token. Such generated identities are not resolved by
  this bounded source query; no compiler-equivalent macro identity is claimed.
- No runtime claim: `exposed` stays a static reading. The matched compiled
  controls are independent evidence for the fixture contract only.

## Acceptance Examples

- Given a trait default `try_get_int` whose changed tail is
  `Ok(sign_extend(self.try_get_uint(nbytes)?, nbytes))`, an
  `impl Buf for &[u8]`, and a test importing `Buf` that binds
  `let mut a = &[0xff, 0xff, 0xff][..];` and asserts
  `assert_eq!(a.try_get_int(3), Ok(-1))`, when `ripr check` runs, then the
  finding reads `exposed`.
- Given the same owner and a test asserting only
  `assert_eq!(short.try_get_int(4), Err(TryGetError { .. }))`, the finding
  stays below `exposed`.
- Given `impl Codec { fn decode(input: u32) -> u32 }` and a free
  `fn decode`, a test asserting `assert_eq!(decode(8), 9)` does not confirm
  a change to `Codec::decode`.
- Given a trait default `next_word` and `impl Reader for Fixed` overriding
  it, a test asserting `assert_eq!(reader.next_word(), 7)` on a `Fixed`
  receiver does not confirm a change to the default.

## Test Mapping

- Unit (`crates/ripr/src/analysis/classify/owner_pin/tests.rs`): the bytes
  shape; trait scope; competing definitions (override, inherent twin,
  `macro_rules!` body, forward, free twin); receiver typing; slice-method
  names; inherent receivers; bare calls (associated twin, free twin, local
  binding, `for`/closure/parameter/macro bindings, `use .. as` renames);
  the return-path gate, including conditionally evaluated tails, spaced
  macros and the sole early `return None;`/`Err` source
  (`an_early_return_is_pinned_when_it_is_the_only_source_of_its_value`,
  `an_early_err_return_needs_to_be_the_only_err_source`,
  `an_early_return_pin_admits_only_the_value_that_return_produces`);
  inline constructor receivers
  (`an_inline_constructor_types_the_receiver_like_a_binding`); module-path
  calls and let-bound results (#6974:
  `a_path_through_the_owners_own_crate_pins_the_owner`,
  `a_path_that_may_leave_the_owners_crate_is_not_a_pin`,
  `a_crate_relative_path_in_another_target_is_not_the_owners_crate`,
  `the_owners_library_name_roots_a_path_from_another_crate`,
  `a_result_bound_once_and_only_asserted_pins_like_the_call`,
  `review_holes_in_path_and_let_bound_pins_stay_closed`,
  `a_test_crate_binding_of_the_library_name_shadows_the_dependency`,
  `a_rename_to_the_owners_name_or_a_type_of_it_defeats_every_path`,
  `test_crate_bindings_of_the_root_are_read_from_that_crate_only`,
  `a_path_must_resolve_to_the_owners_own_module`,
  `rev3_review_false_pins_stay_closed`,
  `an_integration_path_is_closed_to_raw_and_macro_shadows`,
  `a_macro_that_may_emit_the_owners_name_defeats_every_path`,
  `an_expected_binding_ripr_cannot_read_is_not_a_distinct_value`,
  `a_bound_method_result_pins_like_the_method_call`,
  `an_unreadable_expected_binding_is_scanned_only_beside_another_owner_call`,
  `a_cfg_gated_owner_is_not_reached_by_a_path`); crate-local
  bindings in another target
  (`a_crate_local_binding_in_another_target_does_not_reach_the_test`,
  `a_crate_local_site_another_crate_can_compile_stays_workspace_wide`,
  `a_module_child_of_an_ambiguous_include_fragment_stays_workspace_wide`,
  `a_withheld_crate_roots_private_glob_is_routed_by_root`); plain `assert_eq!` against an owner-free value, `#[should_panic]`
  and assertions outside the test body; by-value prelude method names;
  constructor signatures; unit-struct receivers
  (`unit_struct_receiver_is_typed_by_its_own_name`,
  `unit_struct_value_admits_only_spellings_nothing_else_can_bind`,
  `edition_2024_into_future_is_a_by_value_prelude_method`,
  `iterator_by_value_comparisons_are_prelude_methods`,
  `unstable_is_partitioned_custom_default_is_admitted`);
  macro-bound, aliased and parameter receivers;
  lexical fallback; the item-container fact.
- Unit borrowed check-helper controls
  (`crates/ripr/src/analysis/classify/owner_pin/tests/helper_pins.rs`, #6482):
  `an_eagerly_called_local_check_helper_lends_its_assertion` (plain,
  `#[track_caller]`, one eager plus one deferred call, a `for` loop over a
  non-empty constant-row table); `a_helper_call_off_the_eager_path_lends_nothing`
  (a loop over a non-constant iterator, branch,
  uninvoked closure, argument, after `return`, cfg-attributed, `async`
  block, qualified path, macro operand, untrusted macro);
  `a_call_that_may_not_name_the_helper_lends_nothing` (closure and nested-fn
  shadows, `use` in the test, duplicate definition in another module, helper
  outside the test's module);
  `only_a_plain_helper_that_runs_to_its_end_lends_its_assertion` (generic,
  early `return`, a `return` on one match arm, return type, assertion in a
  loop, a match arm or an `if let` branch, untrusted macro, cfg attribute,
  `async fn`); `a_helper_that_rebinds_or_feeds_back_the_owner_is_not_a_pin`
  (owner name as a helper parameter, owner called in a call-site argument);
  `the_indexed_helper_assertion_is_the_admitted_one` (the index's own
  helper crediting yields the coordinate the admission accepts);
  `only_producer_credited_helper_calls_lend_an_assertion` (through
  `build_index`, the table loop reaches `test.assertions` and a directly
  invoked closure does not);
  `a_borrowed_assertion_takes_neither_the_path_nor_the_let_bound_pin`;
  `a_helper_that_names_the_owner_twice_lends_nothing`; and
  `a_helper_scoped_assert_eq_binding_refuses_the_loan` (an empty or
  forwarding `macro_rules! assert_eq` inside the helper's body). Each
  negative test carries the positive control, so the six admission tests
  fail with the admission removed, and removing any one gate fails exactly
  its own test. `the_loan_maps_only_plain_parameters_and_lone_eager_calls`
  pins the loan facts RIPR-SPEC-0186 pairing reads.
- Unit execution and macro context controls: `owner_pin_requires_an_executed_assertion_context`,
  `owner_pin_requires_unambiguous_standard_assert_eq`, `owner_pin_refuses_ambiguous_oracle_coordinates`,
  `owner_pin_macro_ambiguity_in_other_files_and_run_memo`,
  `owner_pin_closure_call_must_share_the_bindings_live_scope`, and
  `shared_return_admission_uses_the_outer_invocation_identity`, and
  `owner_pin_requires_test_item_ancestry_and_enabled_cfg` in the same test module.
- Bitwise tails (#6675): `a_bitwise_or_tail_is_unconditional_but_closures_and_lazy_or_are_not`
  and `bitwise_pipe_reading_distinguishes_operand_position`; the public-API
  controls in `crates/ripr/tests/bitwise_or_return_pin.rs`.
- Clone field pins (#6692): `a_clone_compared_with_its_own_receiver_pins_its_fields`,
  `a_clone_field_pin_needs_derived_equality_and_the_returned_literal` and
  `a_clone_field_pin_needs_a_field_type_that_compares_by_value` and
  `a_clone_field_pin_needs_a_receiver_built_without_the_clone` and
  `a_clone_field_pin_refuses_generic_types_and_instantiated_impls` in the same
  test module; `a_clone_field_owner_pin_is_credited_only_through_reveals_gates`
  in `analysis/classify/reveal.rs`; the public-API controls in
  `crates/ripr/tests/clone_field_whole_equality.rs`.
- CFG authority (`analysis/facts/cfg_predicates/tests.rs`):
  `test_build_availability_preserves_unknown_and_boolean_identity` and
  `test_build_availability_refuses_raw_attribute_heads` distinguish
  enabled, disabled, unknown and malformed inputs without changing role classification.
- Integration (`crates/ripr/tests/owner_pin_execution.rs`):
  `owner_pin_matched_static_and_runtime_controls` (twenty fixtures, two library variants),
  `owner_pin_token_overlap_cannot_bypass_oracle_admission`,
  `owner_pin_shared_admission_keeps_credit_on_one_admitted_oracle` (six mixed cases), and
  `owner_pin_refused_rows_do_not_crowd_out_admitted_oracles` (eight related tests),
  and `owner_pin_review_admission_controls` (six public-API review regressions).
  `local_empty_macro_preserves_independent_equality_execution` compares static
  admission with compiled correct/wrong subjects for local empty, returning,
  imported, ambiguous, shadowed and disabled declarations. The property
  quarantine integration retains its named/direct/helper mixed positives.
  The execution fixtures and their JSON/human outputs are mapped in `.ripr/traceability.toml`.
  `bool_owner_assert_pin_matched_static_and_runtime_controls` runs thirteen
  bool-owner layouts (both boundary sides, let-bound inputs, far inputs
  only, one side only, a shadowing closure, an uncalled closure, a boundary
  call or binding only in a message argument or an operand comment, and a boundary call left
  unasserted on the assertion's line) against the rewrite and a `<`
  mutant; only the two `exposed` layouts fail on the mutant.
  `unit_struct_receiver_matched_static_and_runtime_controls` (#7083) pins a
  kept trait default through `Unit.advance()` and through `let unit = Unit;`,
  and refuses an impl that overrides the default; only the two `exposed`
  layouts fail on a `4 + self.step()` mutant.
  `reexported_value_under_a_unit_struct_name_is_not_credited` imports a
  `pub use std::u32::MAX;` re-export over a unit struct `MAX`, and
  `aliased_outside_module_beside_a_same_named_module_is_not_credited` globs
  `use std::u32 as nums;` beside an unrelated `mod nums`; in both the mutant
  passes and the finding reads `weakly_exposed`.
  `unstable_is_partitioned_custom_default_matched_controls` (#7098 review)
  credits a custom `is_partitioned` default through a receiver that also
  implements `Iterator` — the std method is still unstable on 1.95, so the
  mutant fails; `raw_path_shadow_module_keeps_the_mutant_green` (#7098
  review) loads a rival `Unit` from a `#[r#path]`-named non-`.rs` module,
  and the mutant passes while the finding refuses.
- Bool-owner unit tests: `a_bare_assert_pins_a_bool_owner_to_true_or_false`,
  `a_bare_assert_pins_nothing_on_a_non_bool_owner`,
  `a_bare_assert_keeps_the_owner_binding_defeats`; pairing unit test
  `line_activation_does_not_pair_through_another_owner_call_on_the_line`,
  `comments_and_strings_in_operands_do_not_pair`,
  `quoted_owner_text_on_the_line_keeps_the_activation_fallback`.
- Fixtures: `fixtures/owner_return_pin_trait_method`,
  `fixtures/owner_return_pin_identity_traps`; re-blessed
  `fixtures/infect_value_returned`, `fixtures/infect_wildcard_discard`,
  `fixtures/tail_comparison_boundary`, whose tests pin a free owner's
  unconditional tail.

## Implementation Mapping

- `crates/ripr/src/analysis/syntax/ra.rs`: `function_item_fact` fills
  `FunctionFact.item`.
- `crates/ripr/src/analysis/facts/model.rs`: `FunctionContainer`,
  `FunctionItemFact`.
- `crates/ripr/src/analysis/classify/owner_pin.rs`: `OwnerReturnPin`
  (`establish` for the owner-side gates, `admits` for the test-side gates;
  `OwnerPinSyntax::admits_equality_assertion` applies shared context independently
  of whether an owner-return pin can be established).
- `crates/ripr/src/analysis/classify/reveal.rs`: the pin joins the
  confirmation signals behind the family, oracle-kind and owner-binding
  defeats. `analyze_related_assertions` applies shared context before matching
  or credit and preserves refused assertions only as zero-oracle relations; `file_imports_own_item`; `::`-rooted `use` paths.
- `crates/ripr/src/analysis/classify/operand_pin.rs`: rule 7's
  operand-only pin, applied in `classifier/evidence.rs`.
- `crates/ripr/src/analysis/classifier/evidence.rs`: establishes the pin
  once per probe and supplies the shared context-admission callback.
- `crates/ripr/src/analysis/classifier/finding.rs` and `classify/decision.rs`:
  project the existing stage verdict into execution/binding guidance and
  missing evidence, without changing classification or confidence arithmetic.
- `crates/ripr/src/analysis/syntax/owner_pin.rs`: private bounded assertion
  execution-context query. `local_helper_assertions` and `plain_check_helper`
  own rule 7's context and identity gates; `OwnerPinAssertions::helper_loan`
  reports whether an admitted assertion was borrowed, from which helper, and
  through which eager calls. `OwnerPinSyntax` in the classifier memoizes the
  workspace macro scan and per-file context across probes in one immutable
  index; no output or serialized `OracleFact` field changes.
- `crates/ripr/src/analysis/facts/cfg_predicates.rs`: bounded test-build
  availability reuses the canonical lexer; the existing test-only role query
  retains its separate contract. Out-of-line resolution remains owned by
  existing `FileFacts::role_provenance`, not by the admission consumer.
- `crates/ripr/src/analysis/facts/test_helpers.rs`: same-file helper
  crediting. Rule 7 (#7125) also credits a unique top-level `Production`
  helper in a crate-root integration-test target (`tests/<name>.rs` or
  `tests/<name>/main.rs` relative to the nearest owning manifest,
  including a package nested under `tests/`) without changing its item
  role, and only when that helper's parser-backed oracles include
  `assert_eq!`. `assert!` and harness `.contains()` helpers stay
  uncredited so RIPR-SPEC-0114's last-established edge and
  RIPR-SPEC-0155's harness oracles stay intact. Nested `tests/support/`
  (including `tests/support/tests/`), `benches/`, `examples/` (including
  `examples/tests/`), `src/tests/`, and other production-file helpers stay
  out. Undeclared `tests/*.rs` files left unbuilt by `autotests = false`
  are dropped before this producer (#6965). Nested-package credit
  additionally requires Cargo metadata membership so `[workspace] exclude`
  cannot become test evidence; an established `harness = false` or
  `test = false` target is not credited. Root-package autotest roots
  otherwise stay path-shape.
- `crates/ripr/src/analysis/seam_cache.rs`: classified `1.25`, sharded `0.31`,
  compact `0.31` invalidate stale false credit. File-fact `1.15` from #4748 is preserved;
  the query reads existing indexed source, so no file-fact migration is needed.
  The new classified generation also rejects favorable results from published
  candidate 740098f5, whose post-reveal pairing could re-use refused assertions.
  The statement-prefix refinement changes no serialized fact shape.
  Rule 7 (#6482) moves classified full `1.54`, sharded and compact `0.60`, so
  a warm hit cannot keep a check helper's assertion uncredited; file facts
  are unchanged. Integration-target helpers (#7125) move classified full
  `1.61`, sharded and compact `0.67` so a warm `1.60` hit cannot keep a
  `tests/*.rs` `assert!` / `.contains()` helper over-credited.

## Metrics

- `owner_return_pin_false_exposed` — zero: no fixture or replay finding
  gains `exposed` through the pin unless its confirming assertion is a
  complete call that names the owner on the changed return path.
