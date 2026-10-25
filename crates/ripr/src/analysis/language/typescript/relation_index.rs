//! Inverted relation index: which tests can possibly relate to an owner
//! (#10028).
//!
//! `related_test_candidates` used to run `owner_call_relation` against EVERY
//! workspace test for EVERY owner — O(owners × tests) per-pair guard work
//! (body scans, import normalization). In a test-heavy workspace (thousands
//! of test files) that product dominated: ~0.6 s per owner over 2000 tests.
//!
//! Every pass-1 arm of `owner_call_relation` requires at least one of:
//!
//! 1. the owner name in the test body (direct/receiver/method calls, and any
//!    spy-fabrication or unresolved-ownership reference), or
//! 2. an import record tied to the owner: imported name == owner name
//!    (imported, aliased, re-exported-as, or `default` when the owner is the
//!    default export), a namespace import (member calls carry the name, but a
//!    reference-without-call is conservatively admitted), or a side-effect
//!    import (only ModuleFunction owners can match those), or
//! 3. a re-export chain: the test imports an exposed name X from a barrel
//!    whose chain lands on the owner. The names each owner is exposed under
//!    are derived once from the per-analysis `ReExportIndex`.
//!
//! A test matching NONE of those keys cannot be credited by any pass-1 arm,
//! so restricting the scan to the indexed candidates is a pre-filter, not a
//! semantic change: every identity guard (#4102/#4103 shadowing, mock-module,
//! fabrication, declaration anchors) still runs unchanged on the survivors.
//! The module-entry and heuristic fallback passes only run when pass 1 found
//! nothing and CAN credit name-less tests, so they keep scanning the full
//! workspace slice exactly as before.
//!
//! [`RelationIndex::empty`] (test-only) preserves the full scan so the 1527
//! unit tests that call the pure functions directly need no fixture changes —
//! mirroring the `ReExportIndex::empty()` convention.

use std::collections::{BTreeSet, HashMap, HashSet};

use super::OwnerKind;
use super::related_tests::ReExportIndex;
use super::types::{TypeScriptOwner, TypeScriptTest};

/// Inverted index over the workspace test corpus. Built once per analysis.
#[derive(Debug, Default)]
pub(crate) struct RelationIndex {
    /// Tests that mention a name anywhere in the body (identifier and
    /// string-literal runs, comments included — over-approximation is sound)
    /// or import it (`import { N }`, `import N from`, `import { a as N }`).
    by_name: HashMap<String, Vec<u32>>,
    /// Tests with a default import (`import D from "..."`).
    default_imports: Vec<u32>,
    /// Tests with a bare side-effect import (`import "..."`).
    side_effect_imports: Vec<u32>,
    /// Tests with a namespace import (`import * as ns from "..."`).
    namespace_imports: Vec<u32>,
    /// Owner name -> names re-export chains expose it under. A test importing
    /// ANY of those names passes the re-export arm's name gate; the chain
    /// resolution itself still runs per candidate. Keyed by name only (module
    /// ignored) as a conservative union across same-named owners.
    reexport_exposed: HashMap<String, HashSet<String>>,
    /// True for the test-only empty index: `candidates` yields every test so
    /// behaviour is identical to an unindexed scan.
    scan_all: bool,
}

impl RelationIndex {
    /// Construct an empty index (no pre-filtering).
    /// Used by unit-test callers that exercise the pure scan functions
    /// directly. Test-only: production callers (the live driver) always
    /// build the real index once per analysis.
    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        Self {
            scan_all: true,
            ..Self::default()
        }
    }

    /// Build the index from the workspace test corpus and the per-analysis
    /// re-export graph. Tokenizing is O(total body bytes); the re-export
    /// inversion resolves each (module, exported name) pair once.
    pub(crate) fn build(all_tests: &[TypeScriptTest], reexport_index: &ReExportIndex) -> Self {
        let mut index = Self::default();
        for (i, test) in all_tests.iter().enumerate() {
            let i = i as u32;
            let mut seen: HashSet<&str> = HashSet::new();
            for token in body_name_tokens(&test.body_text) {
                if seen.insert(token) {
                    index.by_name.entry(token.to_string()).or_default().push(i);
                }
            }
            for import in &test.imports_in_file {
                if import.namespace {
                    index.namespace_imports.push(i);
                } else if import.imported.is_none() {
                    index.side_effect_imports.push(i);
                } else if import.imported.as_deref() == Some("default") {
                    index.default_imports.push(i);
                    if let Some(local) = non_empty(&import.local) {
                        index.push_name(local, i);
                    }
                } else if let Some(name) = import.imported.as_deref().and_then(non_empty) {
                    // The imported name is a lookup key even when the local
                    // binding differs (`import { N as local }`): the relation
                    // arms match on the imported name AND on the local, and
                    // the local usually also appears in the body tokens.
                    index.push_name(name, i);
                    if let Some(local) = non_empty(&import.local) {
                        index.push_name(local, i);
                    }
                }
            }
        }
        index.reexport_exposed = reexport_index.exposed_names_by_owner();
        index
    }

    fn push_name(&mut self, name: &str, test: u32) {
        self.by_name.entry(name.to_string()).or_default().push(test);
    }

    /// Sorted, de-duplicated indices of every test that can possibly relate
    /// to `owner` through a name, import, or re-export key. The empty index
    /// yields every index in `0..all_tests_len`, preserving the unindexed
    /// scan for unit tests.
    pub(crate) fn candidate_indices(
        &self,
        owner: &TypeScriptOwner,
        all_tests_len: usize,
    ) -> Vec<u32> {
        if self.scan_all {
            return (0..all_tests_len as u32).collect();
        }
        let mut set: BTreeSet<u32> = BTreeSet::new();
        if let Some(indices) = self.by_name.get(&owner.name) {
            set.extend(indices.iter().copied());
        }
        // Receiver/construction arms: a Method owner runs when the test
        // constructs its class (`new ClassName(`, an aliased named import,
        // `ns.Class(`) or a ClassMethod call names the class; the import
        // locals and namespace forms already landed in `by_name` at build
        // time, so the class-name key admits every such test.
        if let Some(class_name) = owner.class_name.as_deref().and_then(non_empty)
            && let Some(indices) = self.by_name.get(class_name)
        {
            set.extend(indices.iter().copied());
        }
        if owner.exported_as_default || owner.class_default_export {
            // A default import is the only way to reach a default-exported
            // declaration (or construct a default-exported class) from
            // another module; the guards verify the import source identity.
            set.extend(self.default_imports.iter().copied());
        }
        if owner.exported_as_default {
            // Arms that reach a default-exported owner WITHOUT naming it:
            // the fabrication guard matches a fabricated
            // `spyOn(ns, "default")` (body token "default", no import record
            // for dynamic imports), and re-export chains land on the local
            // name "default" (`export { default as fp }` in a barrel records
            // the exposure under "default", not owner.name). Admit both
            // key sets; the relation guards still run per candidate.
            if let Some(indices) = self.by_name.get("default") {
                set.extend(indices.iter().copied());
            }
            if let Some(exposed) = self.reexport_exposed.get("default") {
                for name in exposed {
                    if let Some(indices) = self.by_name.get(name) {
                        set.extend(indices.iter().copied());
                    }
                }
            }
        }
        if owner.owner_kind == OwnerKind::ModuleFunction {
            set.extend(self.side_effect_imports.iter().copied());
        }
        // Namespace imports can alias a member reference without a call of
        // the bare name; admit them for every owner (bounded by how many
        // tests use namespace imports at all).
        set.extend(self.namespace_imports.iter().copied());
        if let Some(exposed) = self.reexport_exposed.get(&owner.name) {
            for name in exposed {
                if let Some(indices) = self.by_name.get(name) {
                    set.extend(indices.iter().copied());
                }
            }
        }
        set.into_iter().collect()
    }

    /// Every test that can possibly relate to `owner`. The empty index yields
    /// the full slice, preserving the unindexed scan for unit tests.
    pub(crate) fn candidates<'a>(
        &self,
        owner: &TypeScriptOwner,
        all_tests: &'a [TypeScriptTest],
    ) -> Vec<&'a TypeScriptTest> {
        self.candidate_indices(owner, all_tests.len())
            .into_iter()
            .map(|i| &all_tests[i as usize])
            .collect()
    }
}

fn non_empty(name: &str) -> Option<&str> {
    if name.is_empty() { None } else { Some(name) }
}

/// Identifier runs of a test body, including string-literal contents and
/// comment text: a deliberate over-approximation of "mentions a name" that
/// keeps the index sound for the call arms, the spy-fabrication string
/// literal (`spyOn(obj, "name")`) and the unresolved-ownership reference
/// check without context-aware parsing.
fn body_name_tokens(body: &str) -> impl Iterator<Item = &str> {
    body.split(|ch: char| !(ch.is_alphanumeric() || ch == '_' || ch == '$'))
        .filter(|token| !token.is_empty())
}

#[cfg(test)]
mod tests {
    use super::super::admission::TypeScriptAssertionAdmission;
    use super::super::types::{TypeScriptImport, TypeScriptMethodKind};
    use super::*;
    use std::path::PathBuf;

    fn test_with_body(file: &str, body: &str, imports: Vec<TypeScriptImport>) -> TypeScriptTest {
        TypeScriptTest {
            name: file.to_string(),
            local_name: file.to_string(),
            describe_names: Vec::new(),
            file: PathBuf::from(file),
            line: 1,
            body_text: body.to_string(),
            reach_body_text: None,
            assertions: Vec::new(),
            mocks_in_file: Vec::new(),
            imports_in_file: imports,
            unstable_member_roots_in_file: Vec::new(),
            scope_bindings: Vec::new(),
            identity_helpers_in_file: Vec::new(),
            default_member_only_locals_in_file: Vec::new(),
            written_locals_in_file: Vec::new(),
            mock_factories_in_file: Vec::new(),
            mock_spread_sources_in_file: Vec::new(),
            assertion_admission: TypeScriptAssertionAdmission::Unresolved,
            conditional_registration: None,
            factory_call_line: None,
            setup_hook_bodies: Vec::new(),
        }
    }

    fn import(source: &str, imported: Option<&str>, local: &str) -> TypeScriptImport {
        TypeScriptImport {
            source: source.to_string(),
            imported: imported.map(str::to_string),
            local: local.to_string(),
            namespace: false,
        }
    }

    fn owner(name: &str) -> TypeScriptOwner {
        TypeScriptOwner {
            name: name.to_string(),
            file: PathBuf::from("src/lib.ts"),
            start_line: 1,
            end_line: 10,
            owner_kind: OwnerKind::Function,
            class_name: None,
            decorated: false,
            exported_as_default: false,
            imports: Vec::new(),
            params: Vec::new(),
            method_kind: TypeScriptMethodKind::Ordinary,
            class_default_export: false,
            default_object_member: false,
            commonjs_default_export: false,
            commonjs_whole_function: false,
            module_entries: Vec::new(),
            arity: None,
            source_text: None,
        }
    }

    #[test]
    fn candidates_match_body_name_import_and_alias() {
        let tests = vec![
            test_with_body("a.test.ts", "f1(1);", vec![]),
            test_with_body(
                "b.test.ts",
                "other()",
                vec![import("./src/lib", Some("f1"), "renamed")],
            ),
            test_with_body("c.test.ts", "nothing here", vec![]),
        ];
        let index = RelationIndex::build(&tests, &ReExportIndex::default());
        let owner = owner("f1");
        let candidates = index.candidates(&owner, &tests);
        let files: Vec<_> = candidates
            .iter()
            .map(|t| t.file.to_string_lossy().into_owned())
            .collect();
        assert_eq!(files, vec!["a.test.ts", "b.test.ts"]);
    }

    #[test]
    fn default_side_effect_and_namespace_imports_are_admitted() {
        let tests = vec![
            test_with_body("d.test.ts", "", vec![import("./m", Some("default"), "D")]),
            test_with_body("s.test.ts", "", vec![import("./m", None, "")]),
            test_with_body(
                "n.test.ts",
                "",
                vec![TypeScriptImport {
                    source: "./m".to_string(),
                    imported: None,
                    local: "ns".to_string(),
                    namespace: true,
                }],
            ),
            test_with_body("z.test.ts", "unrelated", vec![]),
        ];
        let index = RelationIndex::build(&tests, &ReExportIndex::default());

        let mut default_owner = owner("anything");
        default_owner.exported_as_default = true;
        let files: Vec<_> = index
            .candidates(&default_owner, &tests)
            .iter()
            .map(|t| t.file.to_string_lossy().into_owned())
            .collect();
        assert_eq!(files, vec!["d.test.ts", "n.test.ts"]);

        let mut module_owner = owner("init");
        module_owner.owner_kind = OwnerKind::ModuleFunction;
        let files: Vec<_> = index
            .candidates(&module_owner, &tests)
            .iter()
            .map(|t| t.file.to_string_lossy().into_owned())
            .collect();
        assert_eq!(files, vec!["s.test.ts", "n.test.ts"]);
    }

    #[test]
    fn empty_index_scans_everything() {
        let tests = vec![test_with_body("a.test.ts", "x", vec![])];
        let index = RelationIndex::empty();
        assert_eq!(index.candidates(&owner("nobody"), &tests).len(), 1);
    }

    #[test]
    fn default_owner_admits_fabricated_default_spy_and_barrel_alias() {
        // A fabricated `vi.spyOn(ns, "default")` and a barrel
        // `export { default as fp }` alias both reach a default-exported
        // owner without ever naming it; the pre-filter must admit both or
        // the indexed scan drops relations the full scan credits.
        let tests = vec![
            test_with_body(
                "spy.test.ts",
                "vi.spyOn(mod, \"default\").mockReturnValue(1);",
                vec![],
            ),
            test_with_body(
                "barrel.test.ts",
                "fp();",
                vec![import("./barrel", Some("fp"), "fp")],
            ),
            test_with_body("quiet.test.ts", "unrelated()", vec![]),
        ];
        let reexport = ReExportIndex::from_parts(
            vec![(
                ("barrel".to_string(), "fp".to_string()),
                ("default".to_string(), "src/lib".to_string()),
            )],
            vec![],
            vec![("src/lib".to_string(), "default".to_string())],
        );
        let index = RelationIndex::build(&tests, &reexport);
        let mut default_owner = owner("formatPrice");
        default_owner.exported_as_default = true;
        let files: Vec<_> = index
            .candidates(&default_owner, &tests)
            .iter()
            .map(|t| t.file.to_string_lossy().into_owned())
            .collect();
        assert_eq!(files, vec!["spy.test.ts", "barrel.test.ts"]);
    }
}
