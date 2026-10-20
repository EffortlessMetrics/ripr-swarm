//! Shared data types for the TypeScript preview adapter.

use super::*;

/// Owner extracted from a TypeScript / JavaScript source file.
///
/// Covers the syntax-first owner kinds accepted for the preview surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeScriptOwner {
    pub(crate) name: String,
    pub(crate) file: PathBuf,
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
    pub(crate) owner_kind: OwnerKind,
    pub(crate) class_name: Option<String>,
    pub(crate) decorated: bool,
    /// `true` when the owner declaration is the module's default export
    /// (`export default function name(...)`, `export default const name = ...`).
    /// A default import (`import local from './owner'`) binds exactly this
    /// declaration, so the relation gate may credit a call through `local`
    /// only when this fact is recorded (#4103 under-credit: a named default
    /// export imported under a different local name was previously
    /// unreachable from every relation arm).
    pub(crate) exported_as_default: bool,
    pub(crate) imports: Vec<TypeScriptImport>,
    /// Positional parameter names of the owner function, in signature order
    /// (issue #4102). The single parameter-name list shared by the boundary
    /// witness, shadow guards, and relation guards. Empty means the adapter
    /// recorded no parameter facts: either the owner is not a callable with a
    /// fixed positional signature (module initializer, computed method) or
    /// the signature uses patterns the syntax-first extractor refuses to
    /// summarize (destructuring, rest). Fail-closed pairing with `arity`:
    /// non-empty exactly when `arity` is `Some`, with `params.len()` equal to
    /// it; with no facts the position checks keep the position-blind
    /// behaviour.
    pub(crate) params: Vec<String>,
    /// Method-shape refinement for `OwnerKind::Method` owners: a getter is
    /// invoked by property READS on a receiver, and the constructor runs on
    /// every `new ClassName(...)` — both change which relation needle is
    /// honest for the owner (#4104-B).
    pub(crate) method_kind: TypeScriptMethodKind,
    /// `true` when the owner's CONTAINING CLASS is the module's default
    /// export (`export default class Cart { ... }`). A class default export
    /// has no class-level owner entry — only its methods are indexed — so
    /// each method carries this marker instead, and `exported_as_default`
    /// stays false for them: a method is not itself the module's default
    /// export. Constructor matching uses it to credit
    /// `new <default-import local>(...)` (#4104-B, review #4138).
    pub(crate) class_default_export: bool,
    /// `true` when the module's `export default { name }` (or `{ name: name
    /// }`) object holds this top-level function under its own name, so a
    /// default import `utils` calls it as `utils.name(...)`. Set only for a
    /// literal made of plain same-name entries (no spread, computed key,
    /// method, getter or duplicate), which keeps the member from being
    /// replaced inside the object. RIPR-SPEC-0243 rule 5.
    pub(crate) default_object_member: bool,
    /// Exported names of the owner's module whose code reaches this
    /// top-level function through a bounded same-module call graph (an
    /// exported wrapper, or a value a same-module factory built). Empty for
    /// methods and for owners no other export reaches. See `module_entries`.
    pub(crate) module_entries: Vec<TypeScriptModuleEntry>,
    /// `Some(n)` only when every parameter is a plain binding identifier and
    /// there is no rest parameter; `None` when the list could not be resolved
    /// (destructuring, rest, or extraction unavailable). A boundary witness
    /// may only credit an argument position a parameter could actually read.
    pub(crate) arity: Option<usize>,
    /// The owner's own source text from its declaration start to its end, when
    /// extraction had the containing source. Enables expected-semantics checks
    /// that need the owner body (predicate expected-side liveness, #4102).
    pub(crate) source_text: Option<String>,
}

/// Syntactic method-shape refinement recorded during owner extraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum TypeScriptMethodKind {
    /// An ordinary callable method — invoked only by member CALLS.
    #[default]
    Ordinary,
    /// `get total()` — invoked by property reads (`cart.total`).
    Getter,
    /// `set total(v)` — invoked by property writes (`cart.total = v`).
    Setter,
    /// `constructor(...)` — invoked by `new ClassName(...)`.
    Constructor,
}

impl TypeScriptOwner {
    pub(crate) fn symbol_id(&self) -> SymbolId {
        SymbolId(format!(
            "{}:{}::{}",
            output_language_for(&self.file).as_str(),
            normalized_path(&self.file),
            self.name
        ))
    }
}

/// Test block extracted from a TypeScript / JavaScript test file.
///
/// Covers syntax-first Jest/Vitest `test('name', fn)`, `it('name', fn)`,
/// and array-form `.each(...)('name', fn)` expression statements, including
/// nested `describe(...)` blocks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeScriptTest {
    /// Qualified display name. Nested `describe(...)` names are joined with the
    /// local `test(...)` / `it(...)` name so user surfaces can show context.
    pub(crate) name: String,
    /// The local `test(...)` / `it(...)` string before describe qualification.
    pub(crate) local_name: String,
    /// Nested describe names in outer-to-inner order.
    pub(crate) describe_names: Vec<String>,
    pub(crate) file: PathBuf,
    pub(crate) line: usize,
    pub(crate) body_text: String,
    pub(crate) assertions: Vec<TypeScriptAssertion>,
    /// Module paths referenced by syntactic `vi.mock("...")` /
    /// `jest.mock("...")` calls discovered at the top level of the
    /// containing test file. Populated once per file and cloned into
    /// every `TypeScriptTest` parsed from that file so the classifier
    /// can surface the `mocked_module` static-limit without re-parsing.
    /// Empty when no syntactic mock indirection is present.
    pub(crate) mocks_in_file: Vec<String>,
    /// Runtime imports discovered at the top level of the containing test
    /// file. Used only to map relative named or namespace imports back to a
    /// source owner before considering alias calls related.
    pub(crate) imports_in_file: Vec<TypeScriptImport>,
    /// Names the enclosing scopes bind for this test (declarations,
    /// `beforeEach`/`beforeAll` assignments, callback parameters), each
    /// resolved to its innermost scope. Used only to find receivers built
    /// outside the test body and to detect shadowed constructor names; owner
    /// calls and assertions must still sit in `body_text`.
    pub(crate) scope_bindings: Vec<TypeScriptScopeBinding>,
    /// Test-local identity helpers of the containing test file
    /// (RIPR-SPEC-0249 / RIPR-SPEC-0250, amended 2026-10-15): top-level
    /// `function f(p) { return p; }` or `const f = (p) => p;` declarations
    /// whose name appears nowhere else in the file except as a direct call
    /// `f(`. Populated once per file by `extract_tests`; empty when anything
    /// could rebind or replace the name.
    pub(crate) identity_helpers_in_file: Vec<String>,
    /// `(local, member)` pairs for default imports of the containing test
    /// file whose local appears nowhere in the whole file (setup hooks and
    /// file-level code included) except as its own import binding and as
    /// calls `local.member(`, all of one member (RIPR-SPEC-0243 rule 5).
    /// Populated once per file by `extract_tests`.
    pub(crate) default_member_only_locals_in_file: Vec<(String, String)>,
    /// Test-local mock factories of the containing test file
    /// (RIPR-SPEC-0247 rule 1, amended 2026-10-15): top-level
    /// `function f() { return { ... }; }` or `const f = () => ({ ... });`
    /// declarations without parameters whose name appears nowhere else in
    /// the file except as a direct call `f(`, each paired with the source of
    /// the object literal it returns. Every call builds a fresh object, so a
    /// test-body `const p = f();` is read as that literal. Empty when
    /// anything could rebind or replace the name.
    pub(crate) mock_factories_in_file: Vec<(String, String)>,
    /// Test-file spread sources (RIPR-SPEC-0247 rule 1, amended 2026-10-18):
    /// top-level `const defaults = { key: value, ... };` declarations whose
    /// name appears nowhere else in the file except after a `...` spread,
    /// each paired with the source of its object literal. Nothing can write
    /// the object before a spread copies it, so `{ m: jest.fn(), ...defaults }`
    /// keeps its mock when the literal has no `m` key. Empty when anything
    /// could rebind or replace the name.
    pub(crate) mock_spread_sources_in_file: Vec<(String, String)>,
    /// Whether the test's assertion state is established: a recognized
    /// assertion, an established absence of anything assertion-like, or
    /// something assertion-like RIPR cannot resolve (#5524). An empty
    /// `assertions` vector alone is never authority for "no assertion".
    pub(crate) assertion_admission: TypeScriptAssertionAdmission,
}

/// One name an enclosing test scope binds, resolved to its innermost scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeScriptScopeBinding {
    pub(crate) name: String,
    /// The constructor (`Cart`, `shop.Cart`) `name` holds when a test in the
    /// scope starts: the last hook write in its innermost scope, or else every
    /// declaration there. `None` when that value is anything else, is
    /// ambiguous (a conditional write, or a write that can run between
    /// tests), is only declared, or anything else in the file could rebind
    /// the name or the class.
    pub(crate) constructed_by: Option<String>,
    /// `true` when the innermost binding of `name` is a file-level
    /// declaration. Such a name cannot shadow an import (redeclaring an
    /// imported name is a syntax error); it is the owner itself in a
    /// same-file test. Any other level (a `describe` body or parameter, a
    /// loop header, a hook write, a test callback parameter) shadows an
    /// import or owner of the same name for the test.
    pub(crate) file_level: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeScriptImport {
    pub(crate) source: String,
    pub(crate) imported: Option<String>,
    pub(crate) local: String,
    pub(crate) namespace: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeScriptParseLimit {
    pub(crate) file: PathBuf,
    pub(crate) reason: String,
}

/// A workspace file that could not be read at all (permissions, encoding,
/// transient I/O). Unlike a parse limit the adapter has no syntax facts at
/// all for this path; the file silently vanished from both the owner index
/// and the test index before this record existed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeScriptReadFailure {
    pub(crate) file: PathBuf,
    pub(crate) error: String,
}

/// One concrete, observed gap between the tests a recognized test file
/// registers and the tests the syntax-first extractor actually indexed.
/// Produced by `detect_partial_test_extraction`; consumed by the
/// `typescript_test_extraction_partial` named-limitation producer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeScriptTestExtractionGap {
    pub(crate) file: PathBuf,
    pub(crate) sample_line: usize,
    /// Detected shape, one of `template-literal title`, `tagged-template .each`,
    /// or `test/it call in loop/callback/nested body`.
    pub(crate) shape: &'static str,
    /// Source snippet (single-line, bounded) of the unextracted registration.
    pub(crate) snippet: String,
}

/// Owner-shaped construct the syntax-first owner extractor does not index
/// (#4104-A). One per affected changed file (the first detected shape).
pub(crate) struct TypeScriptOwnerExtractionGap {
    pub(crate) file: PathBuf,
    pub(crate) sample_line: usize,
    /// Detected shape, one of `arrow-function class field`,
    /// `class method with unsupported key`, `class static block`,
    /// `accessor auto-accessor`, `enum declaration`, or
    /// `module/namespace declaration`.
    pub(crate) shape: &'static str,
    /// Source snippet (single-line, bounded) of the unextracted owner shape.
    pub(crate) snippet: String,
}

/// One concrete, observed conditional or loop branch that holds an extracted
/// assertion under a condition static analysis cannot evaluate (#8714).
/// Produced by `detect_conditional_assertion_credit`; consumed by the
/// `typescript_conditional_assertion_credit` named-limitation producer.
/// Classification-neutral, additive disclosure only: the branch recursion is
/// deliberate, and this record carries the caveat so a credited oracle is
/// known to possibly never execute.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeScriptConditionalCreditGap {
    pub(crate) file: PathBuf,
    pub(crate) sample_line: usize,
    /// Bounded single-line source text of the unevaluated condition.
    pub(crate) condition: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypeScriptRelationKind {
    DirectOwnerCall,
    /// Test imports the owner under an explicit `{ OriginalName as local }`
    /// renaming alias from the owner file and calls `local(...)` in the body.
    /// Syntactically equivalent certainty to `DirectOwnerCall`: the rename is
    /// explicit and the body verifiably calls the local alias.
    /// Shadow guard applied: if `local` is re-declared inside the test body,
    /// falls through (not credited at High).
    ImportAliasOwnerCall,
    ImportedOwnerCall,
    ModuleValueReference,
    ReceiverOwnerCall,
    ClassMethodCall,
    /// Test imports a name from a barrel whose explicit in-source
    /// `export { N } from` / `export * from` chain (at most
    /// `MAX_REEXPORT_HOPS` hops, directory specifiers resolved to `index`)
    /// lands on the owner's own export. Deeper, cyclic or ambiguous chains
    /// fail closed.
    ReExportChainFollowed,
    /// Test calls an exported name of the owner's module whose code reaches
    /// the owner through a bounded same-module call graph: an exported
    /// wrapper, or a value a same-module factory built (`export const defu =
    /// createDefu()` whose returned closure calls the changed `_defu`). The
    /// entry call passes the same identity gates as a direct owner call.
    /// Admitted only when no test calls the owner itself. Reach is real but
    /// indirect, so the classifier never promotes it to `exposed`.
    ModuleEntryCall,
    /// Test imports a module-level value (`export const stop =
    /// Symbol('stop')`) from the owner's module and uses it as a value
    /// outside a direct `expect(...)` subject (#8568). Heuristic: the use
    /// shows the test consumes the binding, not that an assertion observes
    /// its initializer, so it never lends oracle credit.
    ModuleValueUse,
    SameFileProximity,
    DescribeName,
    TestName,
}

impl TypeScriptRelationKind {
    pub(crate) fn rank(self) -> u8 {
        match self {
            Self::DirectOwnerCall | Self::ImportAliasOwnerCall => 5,
            Self::ImportedOwnerCall => 4,
            Self::ModuleValueReference => 4,
            Self::ReceiverOwnerCall => 4,
            Self::ClassMethodCall => 4,
            // Re-export chain: same rank as other imported calls — the test
            // genuinely exercises the owner, just via an intermediate file.
            Self::ReExportChainFollowed => 4,
            // Below every relation that calls the owner itself.
            Self::ModuleEntryCall => 3,
            Self::SameFileProximity => 3,
            Self::ModuleValueUse => 2,
            Self::DescribeName => 2,
            Self::TestName => 1,
        }
    }

    pub(crate) fn uses_oracle(self) -> bool {
        matches!(
            self,
            Self::DirectOwnerCall
                | Self::ImportAliasOwnerCall
                | Self::ImportedOwnerCall
                | Self::ModuleValueReference
                | Self::ReceiverOwnerCall
                | Self::ClassMethodCall
                | Self::ReExportChainFollowed
                | Self::ModuleEntryCall
        )
    }

    pub(crate) fn is_uncertain(self) -> bool {
        !self.uses_oracle()
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::DirectOwnerCall => "direct_owner_call",
            Self::ImportAliasOwnerCall => "import_alias_owner_call",
            Self::ImportedOwnerCall => "imported_owner_call",
            Self::ModuleValueReference => "module_value_reference",
            Self::ReceiverOwnerCall => "receiver_owner_call",
            Self::ClassMethodCall => "class_method_call",
            Self::ReExportChainFollowed => "re_export_chain_followed",
            Self::ModuleEntryCall => "module_entry_call",
            Self::ModuleValueUse => "module_value_use",
            Self::SameFileProximity => "same_file_proximity",
            Self::DescribeName => "describe_name",
            Self::TestName => "test_name",
        }
    }
}

/// What one related candidate establishes about the path from its test to
/// the changed owner (#5523).
///
/// [`TypeScriptRelationKind`] stays the relation's provenance: why the test
/// was linked at all. This disposition records what the same relation and
/// identity gates found about whether the test's apparent owner call reaches
/// the changed owner. It is computed once, when the candidate is built, by
/// `owner_path_disposition` in `related_tests.rs`, and consumers read it
/// instead of re-running shadow, import, mock, spy or source-text checks.
///
/// Three groups keep their meaning apart:
///
/// - established or present paths: [`Self::TrustedOwnerPath`] and
///   [`Self::ModuleEntryPath`];
/// - unknown: [`Self::OwnerNameCallUnanchored`], [`Self::HeuristicOnly`] and
///   [`Self::UnresolvedAliasOrReexport`] say nothing about a mismatch;
/// - affirmative mismatch: the four `Rejected*` states are positive evidence
///   that the apparent call does not reach the changed owner.
///
/// The disposition never promotes a relation and never changes its rank or
/// confidence; it only names what the gates already decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypeScriptOwnerPathDisposition {
    /// A trusted relation calls (or reads) the owner itself.
    TrustedOwnerPath,
    /// The test calls a same-module entry that reaches the owner: the path is
    /// present, but whether each test's assertions observe the owner's effect
    /// stays unresolved.
    ModuleEntryPath,
    /// A heuristic-linked test calls a function of the owner's name, none of
    /// the disposition's gates found a mismatch, and no declaration anchors
    /// the name to the owner. A same-name binding in an enclosing `describe`
    /// scope is not one of those gates yet (`enclosing_scope_shadows` only
    /// refuses the direct relation), so such a test also lands here.
    OwnerNameCallUnanchored,
    /// Proximity or name evidence only: the test does not call the owner name.
    HeuristicOnly,
    /// A body-local declaration of the owner name shadows the call.
    RejectedLocalShadow,
    /// An import or body-local destructure binds the owner name to a source
    /// that resolves to a different module or binding.
    RejectedUnrelatedImportOrDestructure,
    /// The test mocks the owner's own module.
    RejectedOwnerModuleMock,
    /// The test spies on the owner name and fabricates the spy's value.
    RejectedSpyFabrication,
    /// The owner name is bound through an import or `require` specifier the
    /// resolver cannot place: neither the owner nor an established other module.
    UnresolvedAliasOrReexport,
}

impl TypeScriptOwnerPathDisposition {
    /// Whether the candidate's assertions may be read as observing an
    /// owner-name call (the legacy `candidate_observes_owner_call` answer).
    ///
    /// Present and unanchored paths are readable; heuristic-only, every
    /// affirmative rejection, and an unresolved binding are not — the
    /// unresolved case keeps today's withheld reading (#5523 changes no
    /// result).
    pub(crate) fn observes_owner_call(self) -> bool {
        match self {
            Self::TrustedOwnerPath | Self::ModuleEntryPath | Self::OwnerNameCallUnanchored => true,
            Self::HeuristicOnly
            | Self::RejectedLocalShadow
            | Self::RejectedUnrelatedImportOrDestructure
            | Self::RejectedOwnerModuleMock
            | Self::RejectedSpyFabrication
            | Self::UnresolvedAliasOrReexport => false,
        }
    }
}

/// Whether one related test exercises a changed predicate boundary (#5527).
///
/// Computed per candidate from the same parsing, owner-call identity,
/// constant resolution and position rules as the finding-wide boundary
/// witness, which is the reduction "any row is `Witnessed`". One row's
/// witness never stands in for another row: a mixed finding keeps a
/// `MissedBoundary` row beside a `Witnessed` one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypeScriptPredicateActivation {
    /// The changed line is not a predicate, so there is no boundary to hit.
    NotApplicable,
    /// A strong, pinned, family-matching assertion observes an owner call
    /// that carries the boundary input, and its expected side is live: the
    /// row's share of the RIPR-SPEC-0027 boundary witness.
    Witnessed,
    /// An owner call in the test carries the boundary input, but no
    /// assertion that could discriminate the change witnesses it (weak or
    /// unpinned, self-comparing, dead expected side, or unobserved call).
    /// The input is present, so this is never a missing input.
    ReachedWithoutDiscriminator,
    /// The owner module pins the boundary input statically (the
    /// `typescript_boundary_input` / `_parameters` fact), and every owner
    /// call the test makes passes a plain integer input off it. The only
    /// state that may support a row-owned missing input.
    MissedBoundary,
    /// Static evidence cannot place the test relative to the boundary: an
    /// untrusted or shadowed owner path, an unparsed or underived boundary,
    /// a computed or absent input, or an owner reference that is not a
    /// plain call. Never a missing input.
    Unresolved,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TypeScriptRelatedCandidate<'a> {
    pub(crate) test: &'a TypeScriptTest,
    pub(crate) relation: TypeScriptRelationKind,
    /// Set only by `related_tests::related_candidate`, from the same gates
    /// that assigned `relation`.
    owner_path: TypeScriptOwnerPathDisposition,
}

impl<'a> TypeScriptRelatedCandidate<'a> {
    pub(super) fn with_owner_path(
        test: &'a TypeScriptTest,
        relation: TypeScriptRelationKind,
        owner_path: TypeScriptOwnerPathDisposition,
    ) -> Self {
        Self {
            test,
            relation,
            owner_path,
        }
    }

    /// What this candidate establishes about the changed owner (#5523).
    pub(crate) fn owner_path(&self) -> TypeScriptOwnerPathDisposition {
        self.owner_path
    }
}

/// Assertion shape extracted from a single `expect(actual).matcher(...)`
/// chain inside a test body.
///
/// `matcher` is the canonical matcher name (`toBe`, `toEqual`, `toThrow`,
/// `toMatchSnapshot`, `toHaveBeenCalledWith`, ...). The full Jest/Vitest
/// matcher surface is large; this preview slice maps the most common
/// matchers to oracle vocabulary and tags the rest as `Unknown`.
/// Async-aware (`.resolves` / `.rejects`) chains are recognised by syntax;
/// custom matchers stay `Unknown`.
///
/// Oracle metadata fields (RIPR-SPEC-0085 §PR5):
/// - `observed_expression` — the `expect(<expr>)` argument as source text.
/// - `expected_value_or_variant` — the matcher argument as source text, only
///   when the argument is a concrete resolvable literal (not dynamic).
///   `None` when the matcher argument is a variable, function call, or other
///   non-literal expression; those cases emit `typescript_dynamic_assertion_unresolved`.
/// - `oracle_confidence` — derived from `oracle_strength` plus whether the
///   expected value is a concrete literal.
/// - `raw_evidence_ref` — `file:line` back to the `expect(...)` call site,
///   i.e. `<file>:<assertion_line>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeScriptAssertion {
    pub(crate) matcher: String,
    pub(crate) argument_count: usize,
    pub(crate) line: usize,
    pub(crate) oracle_kind: OracleKind,
    pub(crate) oracle_strength: OracleStrength,
    pub(crate) mock_payload: Option<TypeScriptMockPayload>,
    pub(crate) error_payload: Option<TypeScriptErrorPayload>,
    /// The `expect(<expr>)` argument as source text (RIPR-SPEC-0085 §PR5).
    pub(crate) observed_expression: Option<String>,
    /// The matcher argument as source text when it is a concrete literal
    /// (RIPR-SPEC-0085 §PR5). `None` for dynamic/non-literal arguments.
    pub(crate) expected_value_or_variant: Option<String>,
    /// Whether the matcher argument is a non-literal dynamic expression
    /// (RIPR-SPEC-0085 §PR5). When `true` and `expected_value_or_variant`
    /// is `None`, the `typescript_dynamic_assertion_unresolved` limitation is emitted.
    pub(crate) has_dynamic_matcher_arg: bool,
    /// Confidence derived from oracle_strength + literal concreteness
    /// (RIPR-SPEC-0085 §PR5).
    pub(crate) oracle_confidence: OracleConfidence,
    /// Rendered oracle text for assertion libraries whose call shape is not
    /// the Jest `expect(...).matcher(...)` form (`assert.strictEqual(...)`,
    /// `expect(...).to.equal(...)`, #4547). `None` keeps the Jest/AVA
    /// rendering in `assertion_oracle_text`.
    pub(crate) rendered_call: Option<String>,
    /// RIPR-SPEC-0247 rule 2 (typed amendment): loose `==` equality
    /// (`equal`, legacy `deepEqual`) left `relational_check` / weak whose
    /// expected argument is a finite number literal such as `0` or `1`.
    /// Only a non-number coerces to such a literal, so the classifier
    /// credits it against an owner declared to return `number`
    /// (`ts_typed_loose_number_facts`).
    pub(crate) loose_number_expected: bool,
    /// Whether this matcher compares objects by reference identity
    /// (`===` / `Object.is`), so RIPR-SPEC-0246 rule 1 can pin a
    /// copy-for-identity change. Built-in `toBe`, `strictEqual`, and AVA
    /// `is` are true. A RIPR-SPEC-0262 credited custom matcher is true
    /// because its `pass` requires `received === expected`. `toEqual` and
    /// other `exact_value` matchers stay false: sharing a kind is not
    /// identity. A refused or non-strict custom matcher stays false.
    pub(crate) compares_reference_identity: bool,
    /// RIPR-SPEC-0250 rule 1 (amended 2026-10-15): the flat literal object a
    /// `toMatchObject(<name>)` argument stands for, when `<name>` is a
    /// test-body `const` the body names only in its binding and that
    /// argument ([`match_object_consts`]). Read only by
    /// `ts_match_object_facts`; `expected_value_or_variant` stays `None`.
    pub(crate) expected_object_const: Option<String>,
}

impl TypeScriptAssertion {
    /// Jest/Vitest `expect(...).<name>` identity matcher. AVA `is` and
    /// `node:assert` `strictEqual` are not included: `expect.extend` can
    /// register those names with a non-identity body.
    pub(crate) fn jest_expect_compares_reference_identity(matcher: &str) -> bool {
        matcher == "toBe"
    }

    /// Receiver assertions that compare objects by identity: AVA `t.is`
    /// (`Object.is`) and tape / node-style `t.strictEqual` (`===`).
    /// `t.equal` and `t.deepEqual` stay false.
    pub(crate) fn ava_compares_reference_identity(matcher: &str) -> bool {
        matches!(matcher, "is" | "strictEqual")
    }

    /// `node:assert` `strictEqual` compares with `===`.
    pub(crate) fn node_assert_compares_reference_identity(matcher: &str) -> bool {
        matcher == "strictEqual"
    }
}

/// Oracle confidence level derived from `oracle_strength` plus whether the
/// expected value is a concrete literal (RIPR-SPEC-0085 §PR5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OracleConfidence {
    /// Strong oracle strength with a concrete literal expected value.
    High,
    /// Strong oracle strength but no resolvable literal expected value.
    Medium,
    /// Weak or smoke oracle strength.
    Low,
    /// Unknown oracle kind or no expected value evidence.
    Unknown,
}

impl OracleConfidence {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeScriptMockPayload {
    pub(crate) target: String,
    pub(crate) expected: String,
    pub(crate) kind: TypeScriptMockPayloadKind,
}

impl TypeScriptMockPayload {
    pub(crate) fn oracle_text(&self) -> String {
        match self.kind {
            TypeScriptMockPayloadKind::CalledWith => {
                format!(
                    "expect({}).toHaveBeenCalledWith({})",
                    self.target, self.expected
                )
            }
            TypeScriptMockPayloadKind::CalledTimes => {
                format!(
                    "expect({}).toHaveBeenCalledTimes({})",
                    self.target, self.expected
                )
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypeScriptMockPayloadKind {
    CalledWith,
    CalledTimes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeScriptErrorPayload {
    pub(crate) expected: String,
    pub(crate) kind: TypeScriptErrorPayloadKind,
}

impl TypeScriptErrorPayload {
    pub(crate) fn oracle_text(&self) -> String {
        match self.kind {
            TypeScriptErrorPayloadKind::ThrowsLiteral
            | TypeScriptErrorPayloadKind::ThrowsObject
            | TypeScriptErrorPayloadKind::ThrowsClass => {
                format!("expect(...).toThrow({})", self.expected)
            }
            TypeScriptErrorPayloadKind::RejectsThrowLiteral
            | TypeScriptErrorPayloadKind::RejectsThrowObject
            | TypeScriptErrorPayloadKind::RejectsThrowClass => {
                format!("await expect(...).rejects.toThrow({})", self.expected)
            }
            TypeScriptErrorPayloadKind::RejectsMatchObject => {
                format!("await expect(...).rejects.toMatchObject({})", self.expected)
            }
            TypeScriptErrorPayloadKind::ChaiThrowLiteral => {
                format!("expect(...).to.throw({})", self.expected)
            }
            TypeScriptErrorPayloadKind::AssertThrowsRegex
            | TypeScriptErrorPayloadKind::AssertThrowsObject => {
                format!("assert.throws(..., {})", self.expected)
            }
            TypeScriptErrorPayloadKind::AssertRejectsRegex
            | TypeScriptErrorPayloadKind::AssertRejectsObject => {
                format!("await assert.rejects(..., {})", self.expected)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypeScriptErrorPayloadKind {
    /// `.toThrow("exact message")` — string literal payload.
    ThrowsLiteral,
    /// `.toThrow({ code: "X" })` — object literal payload with only
    /// literal-valued keys (fail-closed: shorthand / spread / dynamic
    /// computed properties are rejected).
    ThrowsObject,
    /// `.toThrow(SomeError)` or `.toThrow(SomeError.SubClass)` — class /
    /// constructor reference as a safe member path.
    ThrowsClass,
    /// `await expect(...).rejects.toThrow("exact message")`.
    RejectsThrowLiteral,
    /// `await expect(...).rejects.toThrow({ code: "X" })`.
    RejectsThrowObject,
    /// `await expect(...).rejects.toThrow(SomeError)`.
    RejectsThrowClass,
    /// `await expect(...).rejects.toMatchObject({ code: "X" })`.
    RejectsMatchObject,
    /// chai `expect(...).to.throw("message")` (RIPR-SPEC-0243 rule 10).
    ChaiThrowLiteral,
    /// `node:assert` `throws(fn, /^...$/)` (RIPR-SPEC-0243 rule 10).
    AssertThrowsRegex,
    /// `node:assert` `throws(fn, { message: "..." })` (RIPR-SPEC-0243 rule 10).
    AssertThrowsObject,
    /// `node:assert` `rejects(p, /^...$/)` (RIPR-SPEC-0243 rule 10).
    AssertRejectsRegex,
    /// `node:assert` `rejects(p, { message: "..." })` (RIPR-SPEC-0243 rule 10).
    AssertRejectsObject,
}
