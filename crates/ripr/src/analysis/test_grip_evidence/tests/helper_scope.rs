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
        assert_eq!(
            helper_relation, *credited,
            "{owner}: {full:?}; source: {source}"
        );
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

#[test]
fn helper_credit_keeps_exact_static_associated_terminal_owner() -> Result<(), String> {
    for (generics, call, arguments, credited) in [
        ("", "Real::leaf_value()", "", true),
        ("<Real: Foreign>", "Real::leaf_value()", "::<Fake>", false),
        (
            "<Real: Foreign>",
            "crate::Real::leaf_value()",
            "::<Fake>",
            true,
        ),
    ] {
        let source = format!(
            "struct Real;\nimpl Real {{ fn leaf_value() -> Option<u32> {{ Some(7) }} }}\n\
             struct Decoy;\nimpl Decoy {{ fn leaf_value() -> Option<u32> {{ Some(7) }} }}\n\
             trait Foreign {{ fn leaf_value() -> Option<u32> {{ Some(7) }} }}\n\
             struct Fake; impl Foreign for Fake {{}}\n\
             fn read_leaf_value{generics}() -> Option<u32> {{ {call} }}\n\
             #[test] fn observes_real() {{ assert_eq!(read_leaf_value{arguments}(), Some(7)); }}\n"
        );
        assert_exact_helper_owner_credit(
            &source,
            &[
                ("src/lib.rs::impl Real::leaf_value", credited),
                ("src/lib.rs::impl Decoy::leaf_value", false),
            ],
        )?;
    }
    Ok(())
}

#[test]
fn unresolved_trait_helper_terminals_do_not_borrow_namesake_credit() -> Result<(), String> {
    for (declarations, call, owners) in [
        (
            "trait Probe { fn leaf_value() -> Option<u32>; }\nstruct Real;\nimpl Probe for Real { fn leaf_value() -> Option<u32> { Some(7) } }\nstruct Decoy;\nimpl Probe for Decoy { fn leaf_value() -> Option<u32> { Some(7) } }",
            "Real::leaf_value()",
            [
                "src/lib.rs::impl Probe for Real::leaf_value",
                "src/lib.rs::impl Probe for Decoy::leaf_value",
            ],
        ),
        (
            "trait Real { fn leaf_value() -> Option<u32> { Some(7) } }\ntrait Decoy { fn leaf_value() -> Option<u32> { Some(7) } }\nstruct Value; impl Real for Value {} impl Decoy for Value {}",
            "<Value as Real>::leaf_value()",
            ["src/lib.rs::leaf_value#L1", "src/lib.rs::leaf_value#L2"],
        ),
    ] {
        let source = format!(
            "{declarations}\nfn read_leaf_value() -> Option<u32> {{ {call} }}\n#[test] fn observes_real() {{ assert_eq!(read_leaf_value(), Some(7)); }}\n"
        );
        // The bounded helper resolver does not establish trait dispatch. It
        // must not turn that uncertainty into credit for either namesake.
        assert_exact_helper_owner_credit(&source, &[(owners[0], false), (owners[1], false)])?;
    }
    Ok(())
}

#[test]
fn integration_helper_entries_follow_declared_modules_not_file_stems() -> Result<(), String> {
    for support_body in [
        "use memory_fixture::leaf_value; pub fn read_leaf_value() -> Option<u32> { leaf_value() }",
        "use memory_fixture::leaf_value as run; pub fn read_leaf_value() -> Option<u32> { run() }",
    ] {
        for (declaration, import, call, expected) in [
            (
                "mod support;",
                "use support::read_leaf_value;",
                "read_leaf_value()",
                true,
            ),
            (
                "mod support;",
                "use support::read_leaf_value as read;",
                "read()",
                true,
            ),
            ("mod support;", "", "support::read_leaf_value()", true),
            (
                "mod support;",
                "",
                "crate::support::read_leaf_value()",
                true,
            ),
            (
                "",
                "use support::read_leaf_value;",
                "read_leaf_value()",
                false,
            ),
            ("", "", "support::read_leaf_value()", false),
            (
                "mod support;",
                "use support::read_leaf_value;",
                "{ let read_leaf_value = || Some(7); read_leaf_value() }",
                false,
            ),
            (
                "mod support; mod decoy;",
                "use decoy::read_leaf_value;",
                "read_leaf_value()",
                false,
            ),
            (
                "mod support; trait Foreign { fn read_leaf_value() -> Option<u32>; } struct Decoy; impl Foreign for Decoy { fn read_leaf_value() -> Option<u32> { Some(7) } } fn route_read_leaf_value<support: Foreign>() -> Option<u32> { support::read_leaf_value() }",
                "",
                "route_read_leaf_value::<Decoy>()",
                false,
            ),
            (
                "mod support;",
                "",
                "{ mod support { pub fn read_leaf_value() -> Option<u32> { Some(7) } } support::read_leaf_value() }",
                false,
            ),
            (
                "mod support;",
                "",
                "{ mod support { pub fn read_leaf_value() -> Option<u32> { Some(7) } } use support::read_leaf_value as route; route() }",
                false,
            ),
            (
                "mod support; fn route_read_leaf_value<support>() -> Option<u32> { crate::support::read_leaf_value() }",
                "",
                "route_read_leaf_value::<()>()",
                true,
            ),
            (
                "mod support; fn route_read_leaf_value<support>() -> Option<u32> { self::support::read_leaf_value() }",
                "",
                "route_read_leaf_value::<()>()",
                true,
            ),
            (
                "mod support; fn route_read_leaf_value() -> Option<u32> { mod support { pub fn read_leaf_value() -> Option<u32> { Some(7) } } support::read_leaf_value() }",
                "",
                "route_read_leaf_value()",
                false,
            ),
        ] {
            let test_source = format!(
                "{declaration}\n{import}\n#[test] fn observes_leaf() {{ assert_eq!({call}, Some(7)); }}"
            );
            let sources = [
                (
                    PathBuf::from("src/lib.rs"),
                    "pub fn leaf_value() -> Option<u32> { Some(7) }",
                ),
                (PathBuf::from("tests/reads.rs"), test_source.as_str()),
                (PathBuf::from("tests/support/mod.rs"), support_body),
                (
                    PathBuf::from("tests/decoy/mod.rs"),
                    "pub fn read_leaf_value() -> Option<u32> { Some(7) }",
                ),
                // A sibling integration target declaring support cannot grant the
                // current test that module. Physical-file existence is not binding.
                (PathBuf::from("tests/other.rs"), "mod support;"),
            ];
            let index = index_from_files(&sources)?;
            let seams = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index);
            let seam = seams
                .iter()
                .find(|seam| seam.kind() == SeamKind::ReturnValue)
                .ok_or("leaf return seam")?;
            let context = CompactGripContext::new(&index);
            let evidence = evidence_for_seam_with_context(seam, &context);
            let compact = compact_evidence_for_seam(seam, &context);
            assert_eq!(
                evidence
                    .related_tests
                    .iter()
                    .any(|test| test.relation_reason == RelationReason::HelperOwnerCall),
                expected,
                "{test_source}: {evidence:?}"
            );
            assert_eq!(
                evidence.activate.state == StageState::Yes,
                expected,
                "{test_source}"
            );
            assert_eq!(
                compact.activate.state == StageState::Yes,
                expected,
                "compact: {test_source}"
            );
        }
    }
    Ok(())
}

#[test]
fn blanket_generic_nominal_helpers_keep_exact_entries_and_shadow_refusals() -> Result<(), String> {
    for (implementation, call, expected) in [
        (
            "impl<T> Reader<T> { pub fn read_leaf_value() -> Option<u32> { crate::leaf_value() } }",
            "child::Reader::<()>::read_leaf_value()",
            true,
        ),
        (
            "impl<T> Reader<T> { pub fn route_read_leaf_value() -> Option<u32> { Self::read_leaf_value() } fn read_leaf_value() -> Option<u32> { crate::leaf_value() } }",
            "child::Reader::<()>::route_read_leaf_value()",
            true,
        ),
        (
            "impl<T: Copy> Reader<T> { pub fn read_leaf_value() -> Option<u32> { crate::leaf_value() } }",
            "child::Reader::<()>::read_leaf_value()",
            false,
        ),
        (
            "impl<T> Reader<T> where T: Copy { pub fn read_leaf_value() -> Option<u32> { crate::leaf_value() } }",
            "child::Reader::<()>::read_leaf_value()",
            false,
        ),
        (
            "impl Reader<()> { pub fn read_leaf_value() -> Option<u32> { crate::leaf_value() } } impl Reader<u32> { pub fn read_leaf_value() -> Option<u32> { Some(7) } }",
            "child::Reader::<()>::read_leaf_value()",
            false,
        ),
        (
            "impl<T> Reader<T> { pub fn read_leaf_value() -> Option<u32> { crate::leaf_value() } }",
            "{ mod child { pub struct Reader<T>(T); impl<T> Reader<T> { pub fn read_leaf_value() -> Option<u32> { Some(7) } } } child::Reader::<()>::read_leaf_value() }",
            false,
        ),
    ] {
        let source = format!(
            "fn leaf_value() -> Option<u32> {{ Some(7) }}\nmod child {{ pub struct Reader<T>(T); {implementation} }}\n#[test] fn observes_leaf() {{ assert_eq!({call}, Some(7)); }}"
        );
        assert_exact_helper_owner_credit(&source, &[("src/lib.rs::leaf_value", expected)])?;
    }
    Ok(())
}

#[test]
fn helper_local_modules_do_not_borrow_enclosing_module_identity() -> Result<(), String> {
    for call in [
        "helpers::leaf_value()",
        "use helpers::leaf_value as route; route()",
        "use helpers::leaf_value; leaf_value()",
    ] {
        let source = format!(
            "mod helpers {{\n    pub fn leaf_value() -> Option<u32> {{ Some(7) }}\n}}\nmod decoy {{\n    pub fn leaf_value() -> Option<u32> {{ Some(9) }}\n}}\nfn read_leaf_value() -> Option<u32> {{\n    mod helpers {{ pub use crate::decoy::leaf_value; }}\n    {call}\n}}\n#[test] fn observes_local() {{ assert_eq!(read_leaf_value(), Some(9)); }}"
        );
        assert_exact_helper_owner_credit(&source, &[("src/lib.rs::helpers::leaf_value", false)])?;
    }
    Ok(())
}

#[test]
fn raw_import_and_module_spellings_share_exact_binding_identity() -> Result<(), String> {
    for (local, call, expected) in [
        (
            "use crate::other::leaf_value as r#leaf_value;",
            "leaf_value()",
            false,
        ),
        (
            "use crate::other::leaf_value as r#leaf_value;",
            "crate::leaf_value()",
            true,
        ),
        ("use crate::leaf_value as r#route;", "route()", true),
    ] {
        let source = format!(
            "fn leaf_value() -> Option<u32> {{ Some(7) }}\nmod other {{\n    pub fn leaf_value() -> Option<u32> {{ Some(7) }}\n}}\nfn read_leaf_value() -> Option<u32> {{\n    {local}\n    {call}\n}}\n#[test] fn observes_leaf() {{ assert_eq!(read_leaf_value(), Some(7)); }}"
        );
        assert_exact_helper_owner_credit(
            &source,
            &[
                ("src/lib.rs::leaf_value", expected),
                ("src/lib.rs::other::leaf_value", !expected),
            ],
        )?;
    }
    for (call, expected) in [
        ("helpers::leaf_value()", false),
        ("crate::helpers::leaf_value()", true),
    ] {
        let source = format!(
            "mod helpers {{\n    pub fn leaf_value() -> Option<u32> {{ Some(7) }}\n}}\nfn read_leaf_value() -> Option<u32> {{\n    mod r#helpers {{\n        pub fn leaf_value() -> Option<u32> {{ Some(7) }}\n    }}\n    {call}\n}}\n#[test] fn observes_leaf() {{ assert_eq!(read_leaf_value(), Some(7)); }}"
        );
        assert_exact_helper_owner_credit(
            &source,
            &[("src/lib.rs::helpers::leaf_value", expected)],
        )?;
    }
    Ok(())
}

#[test]
fn eager_macro_helper_operands_require_standard_binding_and_executed_position() -> Result<(), String>
{
    for (name, operand) in [
        ("dbg", "leaf_value()"),
        ("format", "\"{:?}\", leaf_value()"),
        ("format_args", "\"{:?}\", leaf_value()"),
        ("vec", "leaf_value()"),
        ("matches", "leaf_value(), Some(_)"),
    ] {
        for (shadow, prefix, expected) in [
            (false, "", true),
            (true, "", false),
            (false, "std::", true),
            (true, "std::", true),
        ] {
            let binding = if shadow {
                format!("macro_rules! {name} {{ ($($t:tt)*) => {{ () }}; }}")
            } else {
                String::new()
            };
            let source = format!(
                "fn leaf_value() -> Option<u32> {{ Some(7) }}\nfn read_leaf_value() -> Option<u32> {{ {binding} let _ = {prefix}{name}!({operand}); Some(7) }}\n#[test] fn observes_leaf() {{ assert_eq!(read_leaf_value(), Some(7)); }}"
            );
            assert_exact_helper_owner_credit(&source, &[("src/lib.rs::leaf_value", expected)])?;
        }
    }
    let source = "fn leaf_value() -> Option<u32> { Some(7) }\nfn read_leaf_value() -> Option<u32> { assert_eq!(0, 0, \"{:?}\", leaf_value()); Some(7) }\n#[test] fn observes_leaf() { assert_eq!(read_leaf_value(), Some(7)); }";
    assert_exact_helper_owner_credit(source, &[("src/lib.rs::leaf_value", false)])
}

#[test]
fn exact_helper_graph_counts_declaration_edges_at_its_boundary() -> Result<(), String> {
    for edges in [4, 5] {
        let mut source = "mod real {\n pub fn leaf_value() -> Option<u32> { Some(7) }\n}\nmod decoy {\n pub fn leaf_value() -> Option<u32> { Some(9) }\n}\n".to_string();
        let helper_name = |hop| format!("{}read_leaf_value", "route_".repeat(edges - hop - 1));
        for hop in 0..edges {
            let call = if hop + 1 == edges {
                "crate::real::leaf_value()".to_string()
            } else {
                format!("{}()", helper_name(hop + 1))
            };
            source.push_str(&format!(
                "fn {}() -> Option<u32> {{\n    {call}\n}}\n",
                helper_name(hop)
            ));
        }
        source.push_str(&format!(
            "#[test] fn observes_leaf() {{ assert_eq!({}(), Some(7)); }}\n",
            helper_name(0)
        ));
        assert_exact_helper_owner_credit(
            &source,
            &[
                ("src/lib.rs::real::leaf_value", edges == 4),
                ("src/lib.rs::decoy::leaf_value", false),
            ],
        )?;
    }
    Ok(())
}

#[test]
fn helper_initializer_binding_distinguishes_expression_and_statement_macros() -> Result<(), String>
{
    for (definition, body, expected) in [
        (
            "macro_rules! result { ($value:expr) => { $value }; }",
            "let read = read(); result!(read)",
            true,
        ),
        (
            "macro_rules! bind_read { () => { fn read() -> Option<u32> { Some(7) } }; }",
            "let read = read(); bind_read!(); read",
            false,
        ),
    ] {
        let source = format!(
            "fn leaf_value() -> Option<u32> {{ Some(7) }}\nuse crate::leaf_value as read;\n{definition}\nfn read_leaf_value() -> Option<u32> {{ {body} }}\n#[test] fn observes_leaf() {{ assert_eq!(read_leaf_value(), Some(7)); }}"
        );
        assert_exact_helper_owner_credit(&source, &[("src/lib.rs::leaf_value", expected)])?;
    }
    Ok(())
}

#[test]
fn integration_function_local_helper_import_uses_enclosing_module_binding() -> Result<(), String> {
    for (body, expected) in [
        (
            "use support::read_leaf_value; assert_eq!(read_leaf_value(), Some(7));",
            true,
        ),
        (
            "use support::read_leaf_value as route; assert_eq!(route(), Some(7));",
            true,
        ),
        (
            "mod support { pub fn read_leaf_value() -> Option<u32> { Some(7) } } use support::read_leaf_value; assert_eq!(read_leaf_value(), Some(7));",
            false,
        ),
        (
            "use support::read_leaf_value; let read_leaf_value = || Some(7); assert_eq!(read_leaf_value(), Some(7));",
            false,
        ),
    ] {
        let tests = format!("mod support;\n#[test] fn observes_leaf() {{\n{body}\n}}");
        let index = index_from_files(&[
            (
                PathBuf::from("src/lib.rs"),
                "pub fn leaf_value() -> Option<u32> { Some(7) }",
            ),
            (PathBuf::from("tests/reads.rs"), &tests),
            (
                PathBuf::from("tests/support/mod.rs"),
                "use memory_fixture::leaf_value;\npub fn read_leaf_value() -> Option<u32> { leaf_value() }",
            ),
        ])?;
        let seams = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index);
        let seam = seams
            .iter()
            .find(|seam| seam.kind() == SeamKind::ReturnValue)
            .ok_or("leaf return seam")?;
        let context = CompactGripContext::new(&index);
        let full = evidence_for_seam_with_context(seam, &context);
        let compact = compact_evidence_for_seam(seam, &context);
        assert_eq!(
            full.related_tests
                .iter()
                .any(|test| test.relation_reason == RelationReason::HelperOwnerCall),
            expected,
            "{body}: {full:?}"
        );
        assert_eq!(compact.activate.state, full.activate.state, "{body}");
    }
    Ok(())
}

#[test]
fn blanket_generic_helper_competition_uses_cross_file_nominal_identity() -> Result<(), String> {
    for (other, expected) in [
        (
            "pub struct Other<T>(T);\nimpl<T> Other<T> { pub fn read_leaf_value() -> Option<u32> { Some(9) } }",
            true,
        ),
        (
            "pub struct Other<T>(T);\nimpl<T> Other<T> { pub fn read_leaf_value(&self) -> Option<u32> { Some(9) } }",
            true,
        ),
        (
            "use crate::Reader;\nimpl<T> Reader<T> { pub fn read_leaf_value() -> Option<u32> { Some(9) } }",
            false,
        ),
        (
            "use crate::Reader as Alias;\nimpl<T> Alias<T> { pub fn read_leaf_value() -> Option<u32> { Some(9) } }",
            false,
        ),
    ] {
        let source = "mod other;\nfn leaf_value() -> Option<u32> { Some(7) }\npub struct Reader<T>(T);\nimpl<T> Reader<T> { pub fn read_leaf_value() -> Option<u32> { leaf_value() } }\nstruct Unrelated;\nimpl Unrelated { fn read_leaf_value(&self) -> Option<u32> { Some(9) } }\n#[test] fn observes_leaf() { assert_eq!(Reader::<()>::read_leaf_value(), Some(7)); }";
        let index = index_from_files(&[
            (PathBuf::from("src/lib.rs"), source),
            (PathBuf::from("src/other.rs"), other),
            // Distinct integration and binary crates cannot contribute rival
            // impls to the library, even when their nominal type is a namesake.
            (
                PathBuf::from("tests/namesake.rs"),
                "struct Reader<T>(T);\nimpl<T> Reader<T> { fn read_leaf_value(&self) -> Option<u32> { Some(9) } }",
            ),
            (
                PathBuf::from("src/bin/namesake.rs"),
                "struct Reader<T>(T);\nimpl<T> Reader<T> { fn read_leaf_value(&self) -> Option<u32> { Some(9) } }\nfn main() {}",
            ),
        ])?;
        let seams = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index);
        let seam = seams
            .iter()
            .find(|seam| {
                seam.owner().ends_with("::leaf_value") && seam.kind() == SeamKind::ReturnValue
            })
            .ok_or("leaf return seam")?;
        let context = CompactGripContext::new(&index);
        let full = evidence_for_seam_with_context(seam, &context);
        let compact = compact_evidence_for_seam(seam, &context);
        assert_eq!(
            full.related_tests
                .iter()
                .any(|test| test.relation_reason == RelationReason::HelperOwnerCall),
            expected,
            "{other}: {full:?}"
        );
        assert_eq!(compact.activate.state, full.activate.state, "{other}");
    }
    Ok(())
}
