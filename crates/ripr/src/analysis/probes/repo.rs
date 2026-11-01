use super::super::rust_index::{
    FunctionSummary, RustIndex, error_path_twins, find_file_facts, find_owner_function,
};
use super::SeededProbe;
use super::expectations::{expected_sinks, required_oracles};
use super::family::family_for_probe_shape;
use super::ids::repo_probe_id;
use crate::analysis::diagnostic_origin::ParserByteSpan;
use crate::domain::{DeltaKind, Probe, SourceLocation};
use std::collections::HashMap;
use std::path::Path;

#[cfg(test)]
pub fn probes_for_repo_file(root: &Path, path: &Path, index: &RustIndex) -> Vec<Probe> {
    probes_for_repo_file_seeded(root, path, index)
        .into_iter()
        .map(|seeded| seeded.probe)
        .collect()
}

pub(crate) fn probes_for_repo_file_seeded(
    root: &Path,
    path: &Path,
    index: &RustIndex,
) -> Vec<SeededProbe> {
    let mut probes = Vec::new();
    let Some(facts) = index.files().get(path) else {
        return probes;
    };

    // #6914: one error behavior, one repo probe.
    let twins = error_path_twins(&facts.probe_shapes, &facts.source);
    for (shape, twin) in facts.probe_shapes.iter().zip(twins) {
        if twin {
            continue;
        }
        let family = family_for_probe_shape(shape.kind);

        // #3284: harness-role functions never enter the production
        // subject inventory. A cfg(test)-module helper inside a
        // production file still contributes probe shapes to the file
        // facts; an owner carrying an unambiguous test/evidence role is
        // skipped, mirroring the diff path and the seam inventory.
        // #9119: a production fn sharing a physical line with a #[test]
        // fn stays production-eligible (fail closed), matching
        // `changed_line_is_test_evidence` in probes/diff.rs.
        let owner_function = find_owner_function(index, path, shape.start_line);
        if owner_function.is_some_and(|function| {
            repo_owner_is_unambiguous_test_evidence(index, path, shape.start_line, function)
        }) {
            continue;
        }
        // Include facts keep their fragment path for source locations, while
        // `facts::resolve_repository_local_includes` rebases the symbol
        // identity to the parent compilation unit. Normalize that identity
        // here so repo probes remain stable across host path separators.
        let owner = owner_function.map(|function| {
            let mut owner = function.id.clone();
            owner.0 = owner.0.replace('\\', "/");
            owner
        });
        // Ordinal 1 here; post-hoc dedup below handles collisions.
        let id = repo_probe_id(path, &family, owner.as_ref(), &shape.text, 1);

        let expected_sinks = expected_sinks(&shape.text, &family);
        let required_oracles = required_oracles(&shape.text, &family);

        let probe = Probe {
            id,
            location: SourceLocation::new(root.join(path), shape.start_line, 1),
            owner,
            family,
            delta: DeltaKind::Unknown,
            before: None,
            after: Some(shape.text.to_string()),
            expression: shape.text.to_string(),
            expected_sinks,
            required_oracles,
        };
        let parser_span = (shape.start_line == shape.end_line)
            .then(|| ParserByteSpan::same_line(&shape.text, shape.start_byte))
            .flatten();
        probes.push(SeededProbe::maybe_with_span(probe, parser_span));
    }

    // Post-hoc collision de-dup: if two probes got the same id, append .2, .3, …
    // to the 2nd+ occurrences.
    let mut seen: HashMap<String, u32> = HashMap::new();
    for seeded in probes.iter_mut() {
        let count = seen.entry(seeded.probe.id.0.clone()).or_insert(0);
        *count += 1;
        if *count > 1 {
            seeded.probe.id.0 = format!("{}.{}", seeded.probe.id.0, count);
        }
    }

    probes
}

/// Whether a repo-mode probe whose `find_owner_function` owner carries an
/// evidence role should still be skipped.
///
/// Ownership is line-granular: a production fn and a `#[test]` fn opening
/// on one physical line both own it, and the later-listed one wins
/// `find_owner_function`. Without columns the line cannot be attributed
/// to the test, so it stays production-eligible (fail closed) rather than
/// silently dropping the production candidate. Only the shared boundary
/// line is ambiguous; later lines inside the nested evidence fn's body
/// are unambiguously test evidence (#9076 / #9119).
///
/// Copied from `changed_line_is_test_evidence` in `probes/diff.rs` and
/// extended here for a production fn that *ends* on the line where a
/// test starts (different start lines). The helper lives in this file so
/// the busy diff-mode lane is untouched.
fn repo_owner_is_unambiguous_test_evidence(
    index: &RustIndex,
    path: &Path,
    line: usize,
    function: &FunctionSummary,
) -> bool {
    if !function.source_role.is_evidence_role() {
        return false;
    }
    let shares_line_with_production = line == function.start_line
        && find_file_facts(index, path).is_some_and(|facts| {
            facts.functions.iter().any(|other| {
                !other.source_role.is_evidence_role()
                    && other.start_line <= line
                    && line <= other.end_line
                    && (other.start_line == function.start_line || other.end_line == line)
            })
        });
    !shares_line_with_production
}

#[cfg(test)]
mod tests {
    use super::super::super::rust_index::{
        FileFacts, FunctionFact, ProbeShapeFact, ProbeShapeKind, RustIndex,
    };
    use super::*;
    use crate::analysis::facts::FunctionSourceRole;
    use crate::domain::{DeltaKind, ProbeFamily, SymbolId};
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    fn function_fact(
        path: &Path,
        id: &str,
        name: &str,
        start_line: usize,
        end_line: usize,
        source_role: FunctionSourceRole,
    ) -> FunctionFact {
        FunctionFact {
            id: SymbolId(id.to_string()),
            name: name.to_string(),
            file: path.to_path_buf(),
            start_line,
            end_line,
            body: String::new().into(),
            calls: vec![],
            returns: vec![],
            literals: vec![],
            source_role,
            attrs: vec![],
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            item: Default::default(),
            impl_context: Default::default(),
        }
    }

    fn predicate_shape(line: usize, text: &str) -> ProbeShapeFact {
        ProbeShapeFact {
            start_line: line,
            end_line: line,
            start_byte: 0,
            end_byte: text.len(),
            kind: ProbeShapeKind::Predicate,
            text: text.into(),
            value_consumed: false,
            source_lines: None,
        }
    }

    fn index_with_functions_and_shapes(
        path: PathBuf,
        functions: Vec<FunctionFact>,
        probe_shapes: Vec<ProbeShapeFact>,
    ) -> RustIndex {
        RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            files: BTreeMap::from([(
                path.clone(),
                FileFacts {
                    path,
                    functions,
                    probe_shapes,
                    ..FileFacts::default()
                },
            )]),
            ..Default::default()
        })
    }

    #[test]
    fn probes_for_repo_file_emits_known_shape_with_owner() {
        let path = PathBuf::from("src/lib.rs");
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            files: BTreeMap::from([(
                path.clone(),
                FileFacts {
                    path: path.clone(),
                    functions: vec![FunctionFact {
                        id: SymbolId("auth::authenticate".to_string()),
                        name: "authenticate".to_string(),
                        file: path.clone(),
                        start_line: 1,
                        end_line: 6,
                        body:
                            "fn authenticate() -> Result<(), AuthError> { Err(AuthError::Revoked) }"
                                .into(),
                        calls: vec![],
                        returns: vec![],
                        literals: vec![],
                        source_role: FunctionSourceRole::Production,
                        attrs: vec![],
                        impl_attrs: Vec::new(),
                        nested_fn_names: Vec::new(),
                        let_bindings: Vec::new(),
                        item: Default::default(),
                        impl_context: Default::default(),
                    }],
                    // Unrecognized wire strings can no longer reach this
                    // function: they fail at the decode boundary (see
                    // probe_shape_kind_rejects_unknown_wire_strings_at_decode).
                    probe_shapes: vec![ProbeShapeFact {
                        start_line: 4,
                        end_line: 4,
                        start_byte: 48,
                        end_byte: 71,
                        kind: ProbeShapeKind::ErrorPath,
                        text: "Err(AuthError::Revoked)".into(),
                        value_consumed: false,
                        source_lines: None,
                    }],
                    ..FileFacts::default()
                },
            )]),
            ..Default::default()
        });

        let probes = probes_for_repo_file(Path::new("workspace"), &path, &index);

        assert_eq!(probes.len(), 1);
        let probe = &probes[0];
        assert_eq!(probe.id.0, "repo-probe:src_lib.rs:error_path:3bf8c64c");
        assert_eq!(probe.family, ProbeFamily::ErrorPath);
        assert_eq!(probe.delta, DeltaKind::Unknown);
        assert_eq!(
            probe.owner,
            Some(SymbolId("auth::authenticate".to_string()))
        );
        assert_eq!(probe.before, None);
        assert_eq!(probe.after, Some("Err(AuthError::Revoked)".to_string()));
        assert!(
            probe
                .required_oracles
                .iter()
                .any(|oracle| oracle == "exact error variant assertion")
        );
    }

    #[test]
    fn probes_for_repo_file_returns_empty_for_unknown_path() {
        let probes = probes_for_repo_file(
            Path::new("workspace"),
            Path::new("src/missing.rs"),
            &RustIndex::default(),
        );
        assert!(probes.is_empty());
    }

    #[test]
    fn probes_for_included_file_keep_fragment_location_and_parent_owner() {
        let fragment = PathBuf::from("src/parser_fragment.rs");
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            files: BTreeMap::from([(
                fragment.clone(),
                FileFacts {
                    path: fragment.clone(),
                    functions: vec![FunctionFact {
                        id: SymbolId(r"src\lib.rs::impl Parser::clamp".to_string()),
                        name: "clamp".to_string(),
                        file: fragment.clone(),
                        start_line: 1,
                        end_line: 4,
                        body: "fn clamp(&self, value: i32) -> i32 { value }".into(),
                        calls: vec![],
                        returns: vec![],
                        literals: vec![],
                        source_role: FunctionSourceRole::Production,
                        attrs: vec![],
                        impl_attrs: Vec::new(),
                        nested_fn_names: Vec::new(),
                        let_bindings: Vec::new(),
                        item: Default::default(),
                        impl_context: Default::default(),
                    }],
                    probe_shapes: vec![ProbeShapeFact {
                        start_line: 2,
                        end_line: 2,
                        start_byte: 36,
                        end_byte: 54,
                        kind: ProbeShapeKind::ErrorPath,
                        text: "value > self.limit".into(),
                        value_consumed: false,
                        source_lines: None,
                    }],
                    ..FileFacts::default()
                },
            )]),
            ..Default::default()
        });

        let probes = probes_for_repo_file(Path::new("workspace"), &fragment, &index);

        assert_eq!(probes.len(), 1);
        assert_eq!(
            probes[0].location.file,
            PathBuf::from("workspace/src/parser_fragment.rs")
        );
        assert_eq!(probes[0].location.line, 2);
        assert_eq!(
            probes[0].owner,
            Some(SymbolId("src/lib.rs::impl Parser::clamp".to_string()))
        );
    }

    /// #9119: a production fn and a `#[test]` fn opening on one physical
    /// line must keep the production probe in repo mode. The last-listed
    /// owner is the test, so a raw evidence-role skip drops it.
    #[test]
    fn repo_mode_keeps_production_probe_sharing_opening_line_with_test() {
        let path = PathBuf::from("src/lib.rs");
        let index = index_with_functions_and_shapes(
            path.clone(),
            vec![
                function_fact(&path, "g", "g", 1, 1, FunctionSourceRole::Production),
                function_fact(&path, "t", "t", 1, 1, FunctionSourceRole::TestAttribute),
                function_fact(
                    &path,
                    "only_test",
                    "only_test",
                    3,
                    4,
                    FunctionSourceRole::TestAttribute,
                ),
            ],
            vec![
                predicate_shape(1, "x > 2"),
                predicate_shape(4, "assert!(true)"),
            ],
        );

        let probes = probes_for_repo_file(Path::new("workspace"), &path, &index);
        assert!(
            probes.iter().any(|probe| probe.expression == "x > 2"),
            "production predicate sharing its opening line with a #[test] fn must seed a repo probe: {probes:?}"
        );
        assert!(
            probes
                .iter()
                .all(|probe| probe.expression != "assert!(true)"),
            "a line owned only by a #[test] fn must stay out of the repo inventory: {probes:?}"
        );
    }

    /// #9119 nested-test control: later lines of a nested test body stay
    /// test evidence even when the opening line is shared with production.
    #[test]
    fn repo_mode_skips_later_lines_of_nested_test_sharing_opening_line() {
        let path = PathBuf::from("src/lib.rs");
        let index = index_with_functions_and_shapes(
            path.clone(),
            vec![
                function_fact(
                    &path,
                    "outer",
                    "outer",
                    1,
                    3,
                    FunctionSourceRole::Production,
                ),
                function_fact(
                    &path,
                    "helper",
                    "helper",
                    1,
                    3,
                    FunctionSourceRole::TestAttribute,
                ),
            ],
            vec![predicate_shape(1, "x > 0"), predicate_shape(2, "n > 1")],
        );

        let probes = probes_for_repo_file(Path::new("workspace"), &path, &index);
        assert!(
            probes.iter().any(|probe| probe.expression == "x > 0"),
            "shared opening line stays production-eligible: {probes:?}"
        );
        assert!(
            probes.iter().all(|probe| probe.expression != "n > 1"),
            "later nested-test body lines stay test evidence: {probes:?}"
        );
    }

    /// #9119: a production fn that ends on the line where a test starts
    /// still owns that boundary line; last-listed start-line ownership
    /// must not drop the production probe.
    #[test]
    fn repo_mode_keeps_production_probe_ending_where_test_starts() {
        let path = PathBuf::from("src/lib.rs");
        let index = index_with_functions_and_shapes(
            path.clone(),
            vec![
                function_fact(&path, "g", "g", 1, 2, FunctionSourceRole::Production),
                function_fact(&path, "t", "t", 2, 4, FunctionSourceRole::TestAttribute),
            ],
            vec![
                predicate_shape(2, "x > 2"),
                predicate_shape(3, "assert!(true)"),
            ],
        );

        let probes = probes_for_repo_file(Path::new("workspace"), &path, &index);
        assert!(
            probes.iter().any(|probe| probe.expression == "x > 2"),
            "production predicate on the line where a test starts must seed a repo probe: {probes:?}"
        );
        assert!(
            probes
                .iter()
                .all(|probe| probe.expression != "assert!(true)"),
            "later test-only lines must stay out of the repo inventory: {probes:?}"
        );
    }
}

#[cfg(test)]
mod cfg_test_leak_tests {
    use super::probes_for_repo_file;
    use std::path::Path;

    #[test]
    fn cfg_test_module_probe_shapes_seed_no_repo_probes() -> Result<(), String> {
        // #3284: harness-only control flow inside an inline #[cfg(test)]
        // module must never enter the production subject inventory. The
        // repo path iterates probe_shapes without an owner-role check, so
        // a cfg(test) helper's shapes leak as repo findings.
        let root = std::env::temp_dir().join(format!(
            "ripr-repo-leak-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='repo-leak'\nversion='0.1.0'\nedition='2024'\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn price(amount: i32) -> i32 {\n    if amount > 100 { amount - 10 } else { amount }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    fn check_price(result: i32, expected: i32) {\n        if result != expected {\n            panic!(\"price mismatch\");\n        }\n    }\n\n    #[test]\n    fn price_at_boundary() {\n        check_price(price(100), 90);\n    }\n}\n",
        )
        .map_err(|error| error.to_string())?;
        let index =
            crate::analysis::facts::build_index(&root, &[std::path::PathBuf::from("src/lib.rs")])
                .map_err(|error| error.to_string())?;
        let lib = Path::new("src/lib.rs");
        let probes = probes_for_repo_file(&root, lib, &index);
        let leaked: Vec<_> = probes
            .iter()
            .filter(|probe| probe.expression.contains("result != expected"))
            .collect();
        assert!(
            leaked.is_empty(),
            "cfg(test) helper probe shapes must not seed repo probes: {leaked:?}"
        );
        assert!(
            probes
                .iter()
                .any(|probe| probe.expression.contains("amount > 100")),
            "production shapes still seed repo probes"
        );
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }
}

#[cfg(test)]
mod verus_region_tests {
    use super::probes_for_repo_file;
    use crate::domain::ProbeFamily;
    use std::path::Path;

    /// RIPR-SPEC-0369: repository mode must not seed expression probes inside
    /// `verus! { .. }` (the macro token tree is not indexed as Rust), while
    /// ordinary Rust beside the block keeps its probe.
    #[test]
    fn repo_mode_verus_region_seeds_no_expression_probes() -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-repo-verus-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='repo-verus'\nversion='0.1.0'\nedition='2024'\n",
        )
        .map_err(|error| error.to_string())?;
        let source = "use vstd::prelude::*;\nverus! {\nfn inc(x: u32) -> (r: u32)\n    requires x < 100,\n    ensures r <= 200,\n{\n    if x > 5 { x + 2 } else { x }\n}\nspec fn adult(age: u32) -> bool { age >= 21 }\n}\nfn check(n: usize) -> bool {\n    n >= 4\n}\n";
        std::fs::write(root.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
        let index =
            crate::analysis::facts::build_index(&root, &[std::path::PathBuf::from("src/lib.rs")])
                .map_err(|error| error.to_string())?;
        let probes = probes_for_repo_file(&root, Path::new("src/lib.rs"), &index);
        let _ = std::fs::remove_dir_all(&root);
        let inside = probes
            .iter()
            .filter(|probe| probe.location.line <= 10)
            .collect::<Vec<_>>();
        assert!(inside.is_empty(), "no probe inside the region: {inside:?}");
        assert!(
            probes
                .iter()
                .any(|probe| probe.location.line == 12 && probe.family == ProbeFamily::Predicate),
            "ordinary fn beside the block keeps its probe: {probes:?}"
        );
        Ok(())
    }
}
