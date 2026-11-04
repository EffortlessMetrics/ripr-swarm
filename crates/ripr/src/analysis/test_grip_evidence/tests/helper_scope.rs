//! RIPR-SPEC-0005 / #12891: helper edges require lexical binding, not spelling.
use super::*;

#[test]
fn free_helper_graph_honors_lexical_path_bindings() -> Result<(), String> {
    for (declaration, call, expected) in [
        ("", "helpers::read_as_object()", true),
        ("<helpers: Foreign>", "helpers::read_as_object()", false),
        ("<r#helpers: Foreign>", "r#helpers::read_as_object()", false),
        (
            "<helpers: Foreign>",
            "crate::helpers::read_as_object()",
            true,
        ),
        (
            "<helpers: Foreign>",
            "self::helpers::read_as_object()",
            true,
        ),
        ("<'helpers>", "helpers::read_as_object()", true),
        ("<const helpers: usize>", "helpers::read_as_object()", true),
        (
            "",
            "{ type helpers = Fake; helpers::read_as_object() }",
            false,
        ),
        (
            "",
            "{ use crate::Fake as helpers; helpers::read_as_object() }",
            false,
        ),
        (
            "",
            "{ mod helpers { pub fn read_as_object() -> Option<u32> { Some(7) } } helpers::read_as_object() }",
            false,
        ),
    ] {
        for wrapper in ["route_read_as_object", "dispatch"] {
            let arguments = if declaration.contains("const") {
                "::<7>"
            } else if declaration.contains("Foreign") {
                "::<Fake>"
            } else {
                ""
            };
            let source = format!(
                "mod helpers {{ pub fn read_as_object() -> Option<u32> {{ Some(7) }} }}\n\
                 trait Foreign {{ fn read_as_object() -> Option<u32> {{ Some(7) }} }}\n\
                 struct Fake; impl Foreign for Fake {{}}\n\
                 fn {wrapper}{declaration}() -> Option<u32> {{ {call} }}\n\
                 #[test] fn decoy() {{ assert_eq!({wrapper}{arguments}(), Some(7)); }}\n"
            );
            let index = index_from_files(&[(PathBuf::from("src/lib.rs"), &source)])?;
            let shared = SharedIndexTables::build(&index);
            let ordinary = helper_owner_calls_by_file_with_fanout(&index, &shared, true);
            let affinity = TargetAffinityOwnerCalls::build(&index, &shared);
            // Judge the producer before relations or scores can mask an unsafe edge.
            if wrapper == "route_read_as_object" {
                let found = ordinary
                    .free_spelling
                    .get(Path::new("src/lib.rs"))
                    .and_then(|helpers| helpers.get(wrapper))
                    .is_some_and(|calls| calls.contains("read_as_object"));
                assert_eq!(found, expected, "ordinary: {declaration} / {call}");
            } else {
                let found = affinity.free_by_module_path.values().any(|helpers| {
                    helpers
                        .get(wrapper)
                        .is_some_and(|calls| calls.contains("read_as_object"))
                });
                assert_eq!(found, expected, "affinity: {declaration} / {call}");
            }
        }
    }
    Ok(())
}

#[test]
fn enclosing_type_parameters_shadow_helper_modules() -> Result<(), String> {
    for wrapper in [
        "impl<helpers: Foreign> Wrap<helpers> { fn route_read_as_object() -> Option<u32> { helpers::read_as_object() } }",
        "trait Wrapper<helpers: Foreign> { fn route_read_as_object() -> Option<u32> { helpers::read_as_object() } }",
    ] {
        let source = format!(
            "mod helpers {{ pub fn read_as_object() -> Option<u32> {{ Some(7) }} }}\n\
             trait Foreign {{ fn read_as_object() -> Option<u32> {{ Some(7) }} }}\n\
             struct Wrap<T>(T);\n{wrapper}\n"
        );
        let index = index_from_files(&[(PathBuf::from("src/lib.rs"), &source)])?;
        let shared = SharedIndexTables::build(&index);
        let graph = helper_owner_calls_by_file_with_fanout(&index, &shared, true);
        assert!(
            !graph.free_spelling.values().any(|helpers| helpers
                .get("route_read_as_object")
                .is_some_and(|calls| calls.contains("read_as_object"))),
            "{wrapper}"
        );
    }
    Ok(())
}

#[test]
fn bound_cross_file_helpers_keep_their_edges() -> Result<(), String> {
    for call in [
        "helpers::read_as_object()",
        "crate::helpers::read_as_object()",
    ] {
        let source =
            format!("mod helpers;\nfn route_read_as_object() -> Option<u32> {{ {call} }}\n");
        let index = index_from_files(&[
            (PathBuf::from("src/lib.rs"), &source),
            (
                PathBuf::from("src/helpers.rs"),
                "pub fn read_as_object() -> Option<u32> { Some(7) }\n",
            ),
        ])?;
        let shared = SharedIndexTables::build(&index);
        let graph = helper_owner_calls_by_file_with_fanout(&index, &shared, true);
        assert!(
            graph
                .free_spelling
                .get(Path::new("src/lib.rs"))
                .and_then(|helpers| helpers.get("route_read_as_object"))
                .is_some_and(|calls| calls.contains("read_as_object")),
            "{call}"
        );
    }
    Ok(())
}

#[test]
fn shadowed_intermediate_cannot_reintroduce_a_free_leaf() -> Result<(), String> {
    for (bound, path, expected) in [
        ("Helpers", "Helpers", false),
        ("helpers", "helpers", false),
        ("Helpers", "crate::helpers", true),
    ] {
        let source = format!(
            "fn seed_value() {{}}\nfn leaf_value() -> Option<u32> {{ Some(7) }}\n\
             mod helpers {{ pub fn read_leaf_value() -> Option<u32> {{ crate::leaf_value() }} }}\n\
             trait Foreign {{ fn read_leaf_value() -> Option<u32> {{ Some(7) }} }}\n\
             fn route_read_leaf_value_seed_value_leaf_value<{bound}: Foreign>() -> Option<u32> {{ seed_value(); {path}::read_leaf_value() }}\n"
        );
        let index = index_from_files(&[(PathBuf::from("src/lib.rs"), &source)])?;
        let shared = SharedIndexTables::build(&index);
        let graph = helper_owner_calls_by_file_with_fanout(&index, &shared, true);
        let found = graph
            .free_spelling
            .get(Path::new("src/lib.rs"))
            .and_then(|helpers| helpers.get("route_read_leaf_value_seed_value_leaf_value"))
            .is_some_and(|calls| calls.contains("leaf_value"));
        assert_eq!(found, expected, "{bound} / {path}");
    }
    Ok(())
}

#[test]
fn imported_helper_bindings_do_not_restore_shadowed_calls() -> Result<(), String> {
    for (import, body, expected) in [
        (
            "use crate::helpers::read_as_object;",
            "read_as_object()",
            true,
        ),
        (
            "use crate::helpers::read_as_object;",
            "let read_as_object = || Some(7); read_as_object()",
            false,
        ),
        (
            "use crate::helpers::read_as_object as inspect;",
            "inspect()",
            true,
        ),
        (
            "use crate::helpers::read_as_object as inspect;",
            "let inspect = inspect(); inspect",
            true,
        ),
        (
            "use crate::helpers::read_as_object as inspect;",
            "let inspect = || Some(7); let output = inspect(); output",
            false,
        ),
        (
            "use crate::helpers::read_as_object as inspect;",
            "let inspect = || Some(7); inspect()",
            false,
        ),
        (
            "use crate::helpers::read_as_object;",
            "type helpers = Foreign; helpers::read_as_object()",
            false,
        ),
    ] {
        let source = format!(
            "mod helpers;\n{import}\nfn route_read_as_object() -> Option<u32> {{ {body} }}\n"
        );
        let index = index_from_files(&[
            (PathBuf::from("src/lib.rs"), &source),
            (
                PathBuf::from("src/helpers.rs"),
                "pub fn read_as_object() -> Option<u32> { Some(7) }\n",
            ),
        ])?;
        let shared = SharedIndexTables::build(&index);
        let graph = helper_owner_calls_by_file_with_fanout(&index, &shared, true);
        let found = graph
            .free_spelling
            .get(Path::new("src/lib.rs"))
            .and_then(|helpers| helpers.get("route_read_as_object"))
            .is_some_and(|calls| calls.contains("read_as_object"));
        assert_eq!(found, expected, "{import} / {body}");
    }
    Ok(())
}

#[test]
fn receiver_bound_helper_keeps_its_own_leaf_despite_free_name_collision() -> Result<(), String> {
    let receiver = "pub struct Reader;\nimpl Reader {\n    pub fn read_leaf_value(&self) -> Option<u32> { leaf_value() }\n}\n";
    let unrelated = "mod unrelated {\n    pub fn read_leaf_value() -> Option<u32> { crate::wrong_leaf_value() }\n}\n";
    for declarations in [
        format!("{receiver}{unrelated}"),
        format!("{unrelated}{receiver}"),
    ] {
        let source = format!(
            "fn seed_value() {{}}\nfn leaf_value() -> Option<u32> {{ Some(7) }}\n\
             fn wrong_leaf_value() -> Option<u32> {{ Some(9) }}\n{declarations}\n\
             fn route_read_leaf_value_seed_value() -> Option<u32> {{ seed_value(); Reader.read_leaf_value() }}\n\
             fn dispatch() -> Option<u32> {{ Reader.read_leaf_value() }}\n"
        );
        let index = index_from_files(&[(PathBuf::from("src/lib.rs"), &source)])?;
        let shared = SharedIndexTables::build(&index);
        let graph = helper_owner_calls_by_file_with_fanout(&index, &shared, true);
        let route = graph
            .free_spelling
            .get(Path::new("src/lib.rs"))
            .and_then(|helpers| helpers.get("route_read_leaf_value_seed_value"))
            .ok_or("receiver-bound route must remain indexed")?;
        assert!(route.contains("leaf_value"), "{route:?}");
        assert!(!route.contains("wrong_leaf_value"), "{route:?}");
        assert!(
            !route.contains("read_leaf_value"),
            "method is not the free twin: {route:?}"
        );
        let affinity = TargetAffinityOwnerCalls::build(&index, &shared);
        assert!(affinity.by_module_path.values().any(|helpers| {
            helpers
                .get("dispatch")
                .is_some_and(|calls| calls.contains("read_leaf_value"))
        }));
        assert!(!affinity.free_by_module_path.values().any(|helpers| {
            helpers
                .get("dispatch")
                .is_some_and(|calls| calls.contains("read_leaf_value"))
        }));
    }
    Ok(())
}

#[test]
fn same_named_helper_declarations_do_not_exchange_leaf_payloads() -> Result<(), String> {
    let real = "mod real { pub fn read_leaf_value() -> Option<u32> { crate::leaf_value() } }\n";
    let decoy =
        "mod decoy { pub fn read_leaf_value() -> Option<u32> { crate::wrong_leaf_value() } }\n";
    for declarations in [format!("{real}{decoy}"), format!("{decoy}{real}")] {
        let source = format!(
            "fn seed_value() {{}}\nfn leaf_value() -> Option<u32> {{ Some(7) }}\n\
             fn wrong_leaf_value() -> Option<u32> {{ Some(9) }}\n{declarations}\n\
             fn route_read_leaf_value_seed_value() -> Option<u32> {{ seed_value(); real::read_leaf_value() }}\n"
        );
        let index = index_from_files(&[(PathBuf::from("src/lib.rs"), &source)])?;
        let shared = SharedIndexTables::build(&index);
        let graph = helper_owner_calls_by_file_with_fanout(&index, &shared, true);
        let route = graph
            .free_spelling
            .get(Path::new("src/lib.rs"))
            .and_then(|helpers| helpers.get("route_read_leaf_value_seed_value"))
            .ok_or("bound module helper route must remain indexed")?;
        assert!(route.contains("leaf_value"), "{route:?}");
        assert!(!route.contains("wrong_leaf_value"), "{route:?}");
    }
    Ok(())
}

fn assert_exact_helper_owner_credit(source: &str, owners: &[(&str, bool)]) -> Result<(), String> {
    let file = PathBuf::from("src/lib.rs");
    let index = index_from_files(&[(file.clone(), source)])?;
    let seams = inventory_seams_from_index(std::slice::from_ref(&file), &index);
    let context = CompactGripContext::new(&index);
    for (owner, credited) in owners {
        let seam = seams
            .iter()
            .find(|seam| {
                seam.kind() == SeamKind::ReturnValue
                    && seam.owner() == *owner
                    && seam.expression() == "Some(7)"
            })
            .ok_or_else(|| format!("missing return seam for {owner}"))?;
        let full = evidence_for_seam_with_context(seam, &context);
        let compact = compact_evidence_for_seam(seam, &context);
        let helper_relation = full
            .related_tests
            .iter()
            .any(|test| test.relation_reason == RelationReason::HelperOwnerCall);
        assert_eq!(helper_relation, *credited, "{owner}: {full:?}");
        for (surface, reach, activate) in [
            ("full", full.reach.state, full.activate.state),
            ("compact", compact.reach.state, compact.activate.state),
        ] {
            assert_eq!(
                reach == StageState::Yes,
                *credited,
                "{surface} reach: {owner}"
            );
            assert_eq!(
                activate == StageState::Yes,
                *credited,
                "{surface} activation: {owner}"
            );
        }
    }
    Ok(())
}

#[test]
fn helper_credit_keeps_exact_same_named_terminal_owner() -> Result<(), String> {
    let source = "\
mod real { pub fn leaf_value() -> Option<u32> { Some(7) } }
mod decoy { pub fn leaf_value() -> Option<u32> { Some(7) } }
fn read_leaf_value() -> Option<u32> { real::leaf_value() }
#[test] fn observes_real() { assert_eq!(read_leaf_value(), Some(7)); }
";
    assert_exact_helper_owner_credit(
        source,
        &[
            ("src/lib.rs::real::leaf_value", true),
            ("src/lib.rs::decoy::leaf_value", false),
        ],
    )
}

#[test]
fn qualified_and_imported_entries_select_the_exact_duplicate_helper() -> Result<(), String> {
    for (import, call, credited) in [
        ("", "real::read_leaf_value()", true),
        (
            "use crate::real::read_leaf_value as route;",
            "route()",
            true,
        ),
        (
            "use crate::real::read_leaf_value as route;",
            "{ let route = || Some(7); route() }",
            false,
        ),
    ] {
        let source = format!(
            "\
fn leaf_value() -> Option<u32> {{ Some(7) }}
fn wrong_leaf_value() -> Option<u32> {{ Some(7) }}
mod real {{ pub fn read_leaf_value() -> Option<u32> {{ crate::leaf_value() }} }}
mod decoy {{ pub fn read_leaf_value() -> Option<u32> {{ crate::wrong_leaf_value() }} }}
{import}
#[test] fn observes_real() {{ assert_eq!({call}, Some(7)); }}
"
        );
        assert_exact_helper_owner_credit(
            &source,
            &[
                ("src/lib.rs::leaf_value", credited),
                ("src/lib.rs::wrong_leaf_value", false),
            ],
        )?;
    }
    Ok(())
}

#[test]
fn nominal_associated_helpers_keep_bound_entries_and_hops() -> Result<(), String> {
    for (entry, call, expected) in [
        (
            "fn route_read_leaf_value() -> Option<u32> { Reader::read_leaf_value() }",
            "route_read_leaf_value()",
            true,
        ),
        ("", "Reader::read_leaf_value()", true),
        ("", "crate::Reader::read_leaf_value()", true),
        (
            "fn route_read_leaf_value<Reader: Foreign>() -> Option<u32> { Reader::read_leaf_value() }",
            "route_read_leaf_value::<Fake>()",
            false,
        ),
        (
            "fn route_read_leaf_value<Reader: Foreign>() -> Option<u32> { crate::Reader::read_leaf_value() }",
            "route_read_leaf_value::<Fake>()",
            true,
        ),
        ("", "foreign::Reader::read_leaf_value()", false),
        (
            "fn route_read_leaf_value() -> Option<u32> { type Reader = Fake; Reader::read_leaf_value() }",
            "route_read_leaf_value()",
            false,
        ),
    ] {
        let source = format!("\
fn leaf_value() -> Option<u32> {{ Some(7) }}
struct Reader;
impl Reader {{ fn read_leaf_value() -> Option<u32> {{ leaf_value() }} }}
trait Foreign {{ fn read_leaf_value() -> Option<u32> {{ Some(7) }} }}
struct Fake; impl Foreign for Fake {{}}
mod foreign {{ pub struct Reader; impl Reader {{ pub fn read_leaf_value() -> Option<u32> {{ Some(7) }} }} }}
{entry}
#[test] fn observes() {{ assert_eq!({call}, Some(7)); }}
");
        assert_exact_helper_owner_credit(&source, &[("src/lib.rs::leaf_value", expected)])?;
    }
    let source = "\
fn leaf_value() -> Option<u32> { Some(7) }
struct Reader;
impl Reader {
    fn read_leaf_value() -> Option<u32> { leaf_value() }
    fn route_read_leaf_value() -> Option<u32> { Self::read_leaf_value() }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn observes() { assert_eq!(Reader::route_read_leaf_value(), Some(7)); }
}
";
    assert_exact_helper_owner_credit(source, &[("src/lib.rs::leaf_value", true)])
}

#[test]
fn associated_helper_type_paths_keep_real_modules_and_imports() -> Result<(), String> {
    for (import, call) in [
        ("", "helpers::Reader::read_leaf_value()"),
        ("use crate::helpers::Reader;", "Reader::read_leaf_value()"),
    ] {
        let source = format!(
            "\
fn leaf_value() -> Option<u32> {{ Some(7) }}
mod helpers {{
    pub struct Reader;
    impl Reader {{ pub fn read_leaf_value() -> Option<u32> {{ crate::leaf_value() }} }}
}}
{import}
#[test] fn observes() {{ assert_eq!({call}, Some(7)); }}
"
        );
        assert_exact_helper_owner_credit(&source, &[("src/lib.rs::leaf_value", true)])?;
    }
    let index = index_from_files(&[
        (
            PathBuf::from("src/lib.rs"),
            "mod helpers;\nfn leaf_value() -> Option<u32> { Some(7) }\n#[test] fn observes() { assert_eq!(helpers::Reader::read_leaf_value(), Some(7)); }\n",
        ),
        (
            PathBuf::from("src/helpers.rs"),
            "pub struct Reader;\nimpl Reader { pub fn read_leaf_value() -> Option<u32> { crate::leaf_value() } }\n",
        ),
    ])?;
    let seams = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index);
    let seam = seams
        .iter()
        .find(|seam| seam.owner() == "src/lib.rs::leaf_value")
        .ok_or("cross-file associated helper's leaf must be present")?;
    let evidence = evidence_for_seam(seam, &index);
    assert!(
        evidence
            .related_tests
            .iter()
            .any(|test| test.relation_reason == RelationReason::HelperOwnerCall),
        "{evidence:?}"
    );
    assert_eq!(evidence.reach.state, StageState::Yes);
    assert_eq!(evidence.activate.state, StageState::Yes);
    Ok(())
}
