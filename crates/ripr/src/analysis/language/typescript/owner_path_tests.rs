//! Typed owner-path disposition controls (#5523).
//!
//! Each case parses a real owner and a real test source, relates them through
//! the production `related_test_candidates` path, asserts the relation
//! provenance the case depends on, then asserts the typed disposition the
//! candidate carries. Every case also checks that the disposition projects to
//! the same boolean the pre-#5523 formula computed
//! (`legacy_candidate_observes_owner_call`), and that the public row shows an
//! oracle exactly when that boolean holds.

use super::*;

const OWNER_FILE: &str = "src/pricing.ts";
const OWNER_SOURCE: &str = "export function applyDiscount(total: number): number {\n  return total * 0.9;\n}\n\nexport class Cart {\n  items: number[];\n  constructor() {\n    this.items = [];\n  }\n  total(): number {\n    return 1;\n  }\n  static build(): Cart {\n    return new Cart();\n  }\n}\n";
const MERGE_FILE: &str = "src/merge.ts";
const MERGE_SOURCE: &str = "function _merge(base: any, defaults: any): any {\n  const out = { ...defaults };\n  for (const key of Object.keys(base)) {\n    out[key] = base[key];\n  }\n  return out;\n}\n\nexport function createMerge(): (...args: any[]) => any {\n  return (...args) => args.reduce((p, c) => _merge(p, c), {});\n}\n\nexport const merge = createMerge() as (...args: any[]) => any;\n";

/// Which owner of the fixture a case relates to.
#[derive(Clone, Copy)]
enum OwnerPick {
    Named(&'static str, &'static str),
    Constructor,
}

struct Case {
    name: &'static str,
    owner: OwnerPick,
    test_file: &'static str,
    test_source: &'static str,
    reexport_index: fn() -> ReExportIndex,
    relation: TypeScriptRelationKind,
    disposition: TypeScriptOwnerPathDisposition,
    observes: bool,
}

fn no_reexports() -> ReExportIndex {
    ReExportIndex::empty()
}

fn star_barrel() -> ReExportIndex {
    ReExportIndex::from_parts(
        Vec::new(),
        vec![("src/index".to_string(), "src/pricing".to_string())],
        vec![("src/pricing".to_string(), "applyDiscount".to_string())],
    )
}

fn pick_owner(pick: OwnerPick) -> Result<TypeScriptOwner, String> {
    let (file, source) = match pick {
        OwnerPick::Named(file, _) if file == MERGE_FILE => (MERGE_FILE, MERGE_SOURCE),
        _ => (OWNER_FILE, OWNER_SOURCE),
    };
    extract_owners(Path::new(file), source)
        .into_iter()
        .find(|owner| match pick {
            OwnerPick::Named(_, name) => owner.name == name,
            OwnerPick::Constructor => owner.method_kind == TypeScriptMethodKind::Constructor,
        })
        .ok_or_else(|| format!("fixture owner not extracted from {file}"))
}

fn parse_single_test(file: &str, source: &str) -> Result<Vec<TypeScriptTest>, String> {
    let tests = extract_tests(Path::new(file), source);
    if tests.len() != 1 {
        return Err(format!(
            "fixture must parse to one test, got {}: {source}",
            tests.len()
        ));
    }
    Ok(tests)
}

fn check(case: &Case) -> Result<(), String> {
    let owner = pick_owner(case.owner)?;
    let tests = parse_single_test(case.test_file, case.test_source)?;
    let index = (case.reexport_index)();
    let candidates = related_test_candidates(&owner, &tests, None, &index, None);
    let [candidate] = candidates.as_slice() else {
        return Err(format!(
            "{}: expected one candidate, got {}",
            case.name,
            candidates.len()
        ));
    };
    assert_eq!(candidate.relation, case.relation, "{}: relation", case.name);
    assert_eq!(
        candidate.owner_path(),
        case.disposition,
        "{}: disposition",
        case.name
    );
    assert_eq!(
        candidate_observes_owner_call(candidate),
        case.observes,
        "{}: observes",
        case.name
    );
    assert_eq!(
        legacy_candidate_observes_owner_call(candidate, &owner, None, None),
        case.observes,
        "{}: the legacy formula must give the same answer",
        case.name
    );
    // The disposition is the gates' own answer: recomputing it from the
    // candidate's relation reproduces the retained value.
    assert_eq!(
        owner_path_disposition(candidate.test, candidate.relation, &owner, None, None),
        case.disposition,
        "{}: recomputed disposition",
        case.name
    );
    let rows = related_tests_for_candidates(&candidates, None);
    let [row] = rows.as_slice() else {
        return Err(format!("{}: expected one row", case.name));
    };
    assert_eq!(
        row.oracle.is_some(),
        case.observes,
        "{}: a row shows an oracle exactly when the candidate observes the owner",
        case.name
    );
    Ok(())
}

const DIRECT: Case = Case {
    name: "direct owner call",
    owner: OwnerPick::Named(OWNER_FILE, "applyDiscount"),
    test_file: "tests/pricing.test.ts",
    test_source: "import { applyDiscount } from '../src/pricing';\ntest('discounts', () => {\n  expect(applyDiscount(100)).toBe(90);\n});\n",
    reexport_index: no_reexports,
    relation: TypeScriptRelationKind::DirectOwnerCall,
    disposition: TypeScriptOwnerPathDisposition::TrustedOwnerPath,
    observes: true,
};

#[test]
fn owner_path_direct_owner_call_is_trusted() -> Result<(), String> {
    check(&DIRECT)
}

#[test]
fn owner_path_imported_and_aliased_owner_calls_are_trusted() -> Result<(), String> {
    check(&Case {
        name: "import alias owner call",
        test_source: "import { applyDiscount as ad } from '../src/pricing';\ntest('discounts', () => {\n  expect(ad(100)).toBe(90);\n});\n",
        relation: TypeScriptRelationKind::ImportAliasOwnerCall,
        ..DIRECT
    })?;
    check(&Case {
        name: "namespace imported owner call",
        test_source: "import * as pricing from '../src/pricing';\ntest('discounts', () => {\n  expect(pricing.applyDiscount(100)).toBe(90);\n});\n",
        relation: TypeScriptRelationKind::ImportedOwnerCall,
        ..DIRECT
    })
}

#[test]
fn owner_path_receiver_class_method_and_constructor_are_trusted() -> Result<(), String> {
    check(&Case {
        name: "receiver method call",
        owner: OwnerPick::Named(OWNER_FILE, "total"),
        test_source: "import { Cart } from '../src/pricing';\ntest('totals', () => {\n  const cart = new Cart();\n  expect(cart.total()).toBe(1);\n});\n",
        relation: TypeScriptRelationKind::ReceiverOwnerCall,
        ..DIRECT
    })?;
    check(&Case {
        name: "class method call",
        owner: OwnerPick::Named(OWNER_FILE, "build"),
        test_source: "import { Cart } from '../src/pricing';\ntest('builds', () => {\n  expect(Cart.build()).toBeDefined();\n});\n",
        relation: TypeScriptRelationKind::ClassMethodCall,
        ..DIRECT
    })?;
    check(&Case {
        name: "constructor",
        owner: OwnerPick::Constructor,
        test_source: "import { Cart } from '../src/pricing';\ntest('constructs', () => {\n  expect(new Cart().items).toEqual([]);\n});\n",
        relation: TypeScriptRelationKind::ReceiverOwnerCall,
        ..DIRECT
    })
}

#[test]
fn owner_path_reexport_chain_is_trusted() -> Result<(), String> {
    check(&Case {
        name: "star barrel re-export chain",
        test_source: "import { applyDiscount } from '../src/index';\ntest('discounts', () => {\n  expect(applyDiscount(100)).toBe(90);\n});\n",
        reexport_index: star_barrel,
        relation: TypeScriptRelationKind::ReExportChainFollowed,
        ..DIRECT
    })
}

/// `ModuleEntryCall`: the path through `merge` to `_merge` is present, but
/// observation in each test stays unresolved — a distinct state from a
/// trusted owner call, although its assertions stay readable as before.
#[test]
fn owner_path_module_entry_call_is_present_not_trusted() -> Result<(), String> {
    check(&Case {
        name: "module entry call",
        owner: OwnerPick::Named(MERGE_FILE, "_merge"),
        test_file: "tests/merge.test.ts",
        test_source: "import { merge } from '../src/merge';\nit('merges', () => {\n  expect(merge({ a: 1 }, { b: 2 })).toEqual({ a: 1, b: 2 });\n});\n",
        relation: TypeScriptRelationKind::ModuleEntryCall,
        disposition: TypeScriptOwnerPathDisposition::ModuleEntryPath,
        observes: true,
        ..DIRECT
    })
}

/// A body-local `function applyDiscount` shadows the call. The test is
/// linked heuristically through its renamed owner import reference; the
/// apparent `applyDiscount(...)` call reaches the local.
#[test]
fn owner_path_local_same_name_declaration_is_rejected() -> Result<(), String> {
    check(&Case {
        name: "local same-name declaration",
        test_source: "import { applyDiscount as ad } from '../src/pricing';\ntest('discounts', () => {\n  function applyDiscount(x: number) { return 42; }\n  expect(ad).toBeDefined();\n  expect(applyDiscount(100)).toBe(42);\n});\n",
        relation: TypeScriptRelationKind::SameFileProximity,
        disposition: TypeScriptOwnerPathDisposition::RejectedLocalShadow,
        observes: false,
        ..DIRECT
    })
}

/// #4103 shape 1: destructuring the owner name from an unrelated module.
#[test]
fn owner_path_unrelated_destructure_is_rejected() -> Result<(), String> {
    check(&Case {
        name: "unrelated destructure",
        test_source: "import { applyDiscount } from '../src/pricing';\ntest('discounts', () => {\n  const { applyDiscount } = require('../src/factory');\n  expect(applyDiscount(100)).toBe(90);\n});\n",
        relation: TypeScriptRelationKind::SameFileProximity,
        disposition: TypeScriptOwnerPathDisposition::RejectedUnrelatedImportOrDestructure,
        observes: false,
        ..DIRECT
    })
}

/// An import binding the owner name to another module never becomes a
/// candidate today: `heuristic_relation_allowed` refuses it before any
/// candidate is built. The disposition gate still names it affirmatively
/// when asked for a heuristic relation, so a future candidate path cannot
/// read it as trusted.
#[test]
fn owner_path_unrelated_import_is_rejected_at_the_gate() -> Result<(), String> {
    let owner = pick_owner(OwnerPick::Named(OWNER_FILE, "applyDiscount"))?;
    let tests = parse_single_test(
        "tests/pricing.test.ts",
        "import { applyDiscount } from '../src/factory';\ntest('discounts', () => {\n  expect(applyDiscount(100)).toBe(90);\n});\n",
    )?;
    let candidates = related_test_candidates(&owner, &tests, None, &ReExportIndex::empty(), None);
    assert!(
        candidates.is_empty(),
        "an unrelated import is refused before a candidate exists: {:?}",
        candidates
            .iter()
            .map(|candidate| candidate.relation)
            .collect::<Vec<_>>()
    );
    let test = tests.first().ok_or("parsed test missing")?;
    assert_eq!(
        owner_path_disposition(
            test,
            TypeScriptRelationKind::SameFileProximity,
            &owner,
            None,
            None
        ),
        TypeScriptOwnerPathDisposition::RejectedUnrelatedImportOrDestructure
    );
    Ok(())
}

#[test]
fn owner_path_owner_module_mock_is_rejected() -> Result<(), String> {
    check(&Case {
        name: "owner-module mock",
        test_source: "import { applyDiscount } from '../src/pricing';\nvi.mock('../src/pricing');\ntest('discounts', () => {\n  expect(applyDiscount(100)).toBe(90);\n});\n",
        relation: TypeScriptRelationKind::SameFileProximity,
        disposition: TypeScriptOwnerPathDisposition::RejectedOwnerModuleMock,
        observes: false,
        ..DIRECT
    })
}

/// #4103 shape 4: a spy on the owner name with a fabricated return value.
#[test]
fn owner_path_fabricated_spy_return_is_rejected() -> Result<(), String> {
    check(&Case {
        name: "fabricated spy return",
        test_source: "import * as pricing from '../src/pricing';\nimport { applyDiscount } from '../src/pricing';\ntest('discounts', () => {\n  vi.spyOn(pricing, 'applyDiscount').mockReturnValue(42);\n  expect(applyDiscount(100)).toBe(42);\n});\n",
        relation: TypeScriptRelationKind::SameFileProximity,
        disposition: TypeScriptOwnerPathDisposition::RejectedSpyFabrication,
        observes: false,
        ..DIRECT
    })
}

/// #4103 shape 1: a bare owner-name call with no declaration anchor is
/// unknown, not a mismatch — its assertions stay readable as before.
#[test]
fn owner_path_unanchored_owner_name_call_stays_unknown() -> Result<(), String> {
    check(&Case {
        name: "unanchored owner-name call",
        test_file: "tests/helper.test.ts",
        test_source: "test('applyDiscount works', () => {\n  expect(applyDiscount(100)).toBe(90);\n});\n",
        relation: TypeScriptRelationKind::TestName,
        disposition: TypeScriptOwnerPathDisposition::OwnerNameCallUnanchored,
        observes: true,
        ..DIRECT
    })
}

/// Proximity and name evidence is not an owner path: each heuristic label
/// links a test that references the owner without calling it.
#[test]
fn owner_path_same_file_describe_and_test_name_heuristics_are_heuristic_only() -> Result<(), String>
{
    let heuristic = Case {
        name: "same-file proximity",
        test_source: "import { applyDiscount } from '../src/pricing';\ntest('exists', () => {\n  expect(applyDiscount).toBeDefined();\n});\n",
        relation: TypeScriptRelationKind::SameFileProximity,
        disposition: TypeScriptOwnerPathDisposition::HeuristicOnly,
        observes: false,
        ..DIRECT
    };
    check(&heuristic)?;
    check(&Case {
        name: "describe name",
        test_file: "tests/helper.test.ts",
        test_source: "import { applyDiscount } from '../src/pricing';\ndescribe('applyDiscount', () => {\n  it('exists', () => {\n    expect(applyDiscount).toBeDefined();\n  });\n});\n",
        relation: TypeScriptRelationKind::DescribeName,
        ..heuristic
    })?;
    check(&Case {
        name: "test name",
        test_file: "tests/helper.test.ts",
        test_source: "import { applyDiscount } from '../src/pricing';\ntest('applyDiscount exists', () => {\n  expect(applyDiscount).toBeDefined();\n});\n",
        relation: TypeScriptRelationKind::TestName,
        ..heuristic
    })
}

/// A `require` alias the resolver cannot place (no alias map) binds the
/// owner name to an unknown module. That is unknown, not an affirmative
/// mismatch; the boolean keeps today's withheld reading.
#[test]
fn owner_path_unresolved_alias_stays_unknown() -> Result<(), String> {
    check(&Case {
        name: "unresolved require alias",
        test_source: "test('discounts', () => {\n  const { applyDiscount } = require('@app/pricing');\n  expect(applyDiscount(100)).toBe(90);\n});\n",
        relation: TypeScriptRelationKind::SameFileProximity,
        disposition: TypeScriptOwnerPathDisposition::UnresolvedAliasOrReexport,
        observes: false,
        ..DIRECT
    })?;
    check(&Case {
        name: "destructure of an unresolved namespace import",
        test_source: "import * as pricing from '@app/pricing';\ntest('discounts', () => {\n  const { applyDiscount } = pricing;\n  expect(applyDiscount(100)).toBe(90);\n});\n",
        relation: TypeScriptRelationKind::SameFileProximity,
        disposition: TypeScriptOwnerPathDisposition::UnresolvedAliasOrReexport,
        observes: false,
        ..DIRECT
    })
}

/// An affirmative rejection wins over an unresolved binding in the same
/// test: the mock is positive evidence, the alias is only unknown.
#[test]
fn owner_path_affirmative_rejection_wins_over_unresolved_alias() -> Result<(), String> {
    check(&Case {
        name: "mock plus unresolved require alias",
        test_source: "vi.mock('../src/pricing');\ntest('discounts', () => {\n  const { applyDiscount } = require('@app/pricing');\n  expect(applyDiscount(100)).toBe(90);\n});\n",
        relation: TypeScriptRelationKind::SameFileProximity,
        disposition: TypeScriptOwnerPathDisposition::RejectedOwnerModuleMock,
        observes: false,
        ..DIRECT
    })
}

/// An unresolved alias IMPORT of the owner name never becomes a candidate
/// today (`heuristic_relation_allowed` refuses it, as it refuses an
/// unrelated import). The gate still keeps it unknown rather than unrelated.
#[test]
fn owner_path_unresolved_alias_import_is_unknown_at_the_gate() -> Result<(), String> {
    let owner = pick_owner(OwnerPick::Named(OWNER_FILE, "applyDiscount"))?;
    let tests = parse_single_test(
        "tests/pricing.test.ts",
        "import { applyDiscount } from '@app/pricing';\ntest('discounts', () => {\n  expect(applyDiscount(100)).toBe(90);\n});\n",
    )?;
    assert!(
        related_test_candidates(&owner, &tests, None, &ReExportIndex::empty(), None).is_empty(),
        "an unresolved alias import is refused before a candidate exists"
    );
    let test = tests.first().ok_or("parsed test missing")?;
    assert_eq!(
        owner_path_disposition(
            test,
            TypeScriptRelationKind::SameFileProximity,
            &owner,
            None,
            None
        ),
        TypeScriptOwnerPathDisposition::UnresolvedAliasOrReexport
    );
    Ok(())
}

/// The projection itself: only present and unanchored paths are readable.
#[test]
fn owner_path_projection_reads_only_present_and_unanchored_paths() {
    use TypeScriptOwnerPathDisposition as D;
    let readable = [
        D::TrustedOwnerPath,
        D::ModuleEntryPath,
        D::OwnerNameCallUnanchored,
    ];
    let withheld = [
        D::HeuristicOnly,
        D::RejectedLocalShadow,
        D::RejectedUnrelatedImportOrDestructure,
        D::RejectedOwnerModuleMock,
        D::RejectedSpyFabrication,
        D::UnresolvedAliasOrReexport,
    ];
    for disposition in readable {
        assert!(disposition.observes_owner_call(), "{disposition:?}");
    }
    for disposition in withheld {
        assert!(!disposition.observes_owner_call(), "{disposition:?}");
    }
}
