//! Generic type parameters occupy a different namespace from bare value calls.
use super::*;

#[test]
fn helper_scope_type_parameters_preserve_bare_imported_function_calls() -> Result<(), String> {
    for declaration in [
        "fn route_read_leaf_value<read_leaf_value>() -> Option<u32> { read_leaf_value() }",
        "struct S<T>(T); impl<read_leaf_value> S<read_leaf_value> { fn route_read_leaf_value() -> Option<u32> { read_leaf_value() } }",
        "trait Route<read_leaf_value> { fn route_read_leaf_value() -> Option<u32> { read_leaf_value() } }",
    ] {
        for renamed in [false, true] {
            let import = if renamed {
                "use crate::read_leaf_value as imported;"
            } else {
                "use crate::read_leaf_value;"
            };
            let declaration = if renamed {
                declaration.replace("{ read_leaf_value() }", "{ imported() }")
            } else {
                declaration.to_owned()
            };
            let source = format!(
                "fn read_leaf_value() -> Option<u32> {{ Some(7) }}\nmod child {{\n{import}\n{declaration}\n}}\n"
            );
            let index = index_from_files(&[(PathBuf::from("src/lib.rs"), &source)])?;
            let shared = SharedIndexTables::build(&index);
            for fanout in [false, true] {
                let graph = helper_owner_calls_by_file_with_fanout(&index, &shared, fanout);
                assert!(
                    graph.free_spelling.values().any(|helpers| helpers
                        .get("route_read_leaf_value")
                        .is_some_and(|calls| calls.contains("read_leaf_value"))),
                    "{declaration}; renamed={renamed}; fanout={fanout}"
                );
            }
            let affinity = TargetAffinityOwnerCalls::build(&index, &shared);
            assert!(
                affinity.free_by_module_path.values().any(|helpers| helpers
                    .get("route_read_leaf_value")
                    .is_some_and(|calls| calls.contains("read_leaf_value"))),
                "affinity: {declaration}; renamed={renamed}"
            );
        }
    }
    Ok(())
}

#[test]
fn helper_scope_type_only_items_preserve_imported_value_calls() -> Result<(), String> {
    for item in [
        "type leaf_value = ();",
        "type r#leaf_value = ();",
        "enum leaf_value { Variant }",
        "trait leaf_value {}",
        "struct leaf_value { field: u32 }",
        "union leaf_value { field: u32 }",
    ] {
        let source = format!(
            "fn leaf_value() -> Option<u32> {{ Some(7) }}\n\
             mod child {{\nuse crate::leaf_value;\n\
             pub fn read_leaf_value() -> Option<u32> {{ {item} leaf_value() }}\n}}\n\
             #[test] fn observes_real() {{ assert_eq!(child::read_leaf_value(), Some(7)); }}\n"
        );
        let index = index_from_files(&[(PathBuf::from("src/lib.rs"), &source)])?;
        let shared = SharedIndexTables::build(&index);
        let graph = helper_owner_calls_by_file_with_fanout(&index, &shared, true);
        assert!(
            graph.free_spelling.values().any(|helpers| helpers
                .get("read_leaf_value")
                .is_some_and(|calls| calls.contains("leaf_value"))),
            "{item}"
        );
        let seams = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index);
        let seam = seams
            .iter()
            .find(|seam| seam.owner() == "src/lib.rs::leaf_value")
            .ok_or("real leaf seam must be present")?;
        assert_eq!(
            evidence_for_seam(seam, &index).reach.state,
            StageState::Yes,
            "{item}"
        );
    }
    Ok(())
}

#[test]
fn helper_scope_value_only_items_preserve_module_paths() -> Result<(), String> {
    for item in [
        "fn helpers() {}",
        "fn r#helpers() {}",
        "const helpers: usize = 7;",
        "static helpers: usize = 7;",
    ] {
        let source = format!(
            "mod helpers {{ pub fn leaf_value() -> Option<u32> {{ Some(7) }} }}\n\
             fn read_leaf_value() -> Option<u32> {{\n{item}\nhelpers::leaf_value()\n}}\n\
             #[test] fn observes_real() {{ assert_eq!(read_leaf_value(), Some(7)); }}\n"
        );
        let index = index_from_files(&[(PathBuf::from("src/lib.rs"), &source)])?;
        let shared = SharedIndexTables::build(&index);
        let graph = helper_owner_calls_by_file_with_fanout(&index, &shared, true);
        assert!(
            graph.free_spelling.values().any(|helpers| helpers
                .get("read_leaf_value")
                .is_some_and(|calls| calls.contains("leaf_value"))),
            "{item}"
        );
        let seams = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index);
        let seam = seams
            .iter()
            .find(|seam| seam.owner() == "src/lib.rs::helpers::leaf_value")
            .ok_or("real module leaf seam must be present")?;
        assert_eq!(
            evidence_for_seam(seam, &index).reach.state,
            StageState::Yes,
            "{item}"
        );
    }
    Ok(())
}

#[test]
fn helper_scope_value_declarations_still_intercept_bare_calls() -> Result<(), String> {
    for (item, call) in [
        ("fn leaf_value() -> Option<u32> { Some(9) }", "leaf_value()"),
        (
            "struct leaf_value(u32);",
            "{ let _ = leaf_value(7); Some(7) }",
        ),
        (
            "const leaf_value: fn() -> Option<u32> = || Some(9);",
            "leaf_value()",
        ),
        (
            "static leaf_value: fn() -> Option<u32> = || Some(9);",
            "leaf_value()",
        ),
    ] {
        let source = format!(
            "fn leaf_value() -> Option<u32> {{ Some(7) }}\n\
             mod child {{\nuse crate::leaf_value;\n\
             pub fn read_leaf_value() -> Option<u32> {{\n{item}\n{call}\n}}\n}}\n"
        );
        let index = index_from_files(&[(PathBuf::from("src/lib.rs"), &source)])?;
        let shared = SharedIndexTables::build(&index);
        let graph = helper_owner_calls_by_file_with_fanout(&index, &shared, true);
        assert!(
            !graph.free_spelling.values().any(|helpers| helpers
                .get("read_leaf_value")
                .is_some_and(|calls| calls.contains("leaf_value"))),
            "{item}"
        );
    }
    Ok(())
}
