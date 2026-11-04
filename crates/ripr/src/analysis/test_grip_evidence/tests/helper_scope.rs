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
