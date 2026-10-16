use super::*;
use crate::analysis::classify::OwnerPinSyntax;

const NAMED_SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/collection_argument_named/input/src/lib.rs"
));

const TRUSTED_ONE: &str = "pub struct Bag { pub items: Vec<u32> }\npub fn add(v: &mut Vec<u32>, x: u32) {\n    v.push(x * 1);\n}\n\n#[cfg(test)]\nmod tests {\n use super::*;\n// standard assert_eq remains admitted\n#[test]\nfn records_0() { let mut b = Bag { items: Vec::new() }; add(&mut b.items, 11); assert_eq!(b.items, vec![11]); }\n}\n";
const SHADOW_ONE: &str = "pub struct Bag { pub items: Vec<u32> }\npub fn add(v: &mut Vec<u32>, x: u32) {\n    v.push(x * 1);\n}\n\n#[cfg(test)]\nmod tests {\n use super::*;\nmacro_rules! assert_eq { ($($args:tt)*) => {} }\n#[test]\nfn records_0() { let mut b = Bag { items: Vec::new() }; add(&mut b.items, 11); assert_eq!(b.items, vec![11]); }\n}\n";

fn syntax_fixture(tests: usize, shadow_standard_root: bool) -> Result<FixtureIndex, String> {
    let (production, _) = NAMED_SOURCE
        .split_once("#[cfg(test)]")
        .ok_or("missing fixture tests")?;
    let mut source = production.to_owned();
    source.push_str("#[cfg(test)]\nmod tests {\n use super::*;\n");
    source.push_str(if shadow_standard_root {
        "macro_rules! assert_eq { ($($args:tt)*) => {} }\n"
    } else {
        "// standard assert_eq remains admitted\n"
    });
    for n in 0..tests {
        source.push_str(&format!("#[test]\nfn records_{n}() {{ let mut b = Bag {{ items: Vec::new() }}; add(&mut b.items, 11); assert_eq!(b.items, vec![11]); }}\n"));
    }
    source.push_str("}\n");
    let fixture_root = authority_fixture_root("syntax-reuse")?;
    fs::create_dir_all(fixture_root.join("src")).map_err(|e| e.to_string())?;
    fs::write(
        fixture_root.join("Cargo.toml"),
        "[package]\nname = \"syntax-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|e| e.to_string())?;
    fs::write(fixture_root.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
    let index = build_index(&fixture_root, &[PathBuf::from("src/lib.rs")])?;
    Ok(FixtureIndex {
        index,
        _fixture_root: fixture_root,
    })
}

fn push_seam(seams: &[RepoSeam]) -> Result<&RepoSeam, String> {
    seams
        .iter()
        .find(|s| {
            s.kind() == SeamKind::SideEffect
                && s.expression().trim().trim_end_matches(';') == "v.push(x * 1)"
        })
        .ok_or_else(|| "missing accepted collection push seam".to_owned())
}

#[test]
fn ranking_reuses_file_syntax_and_rejects_the_fresh_per_test_control() -> Result<(), String> {
    let index = syntax_fixture(12, false)?;
    let seams = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index);
    let seam = push_seam(&seams)?;
    let context = CompactGripContext::new(&index);
    let related = find_related_tests_with_context(seam, &context);
    assert_eq!(related.len(), 12, "every actual test remains selected");
    let owner_fn = context.owner_function(seam.file(), seam.display_line());
    let owner = seam_owner_activation(seam, &context);
    let shared = OwnerPinSyntax::default();
    let started = std::time::Instant::now();
    let before = OwnerPinSyntax::std_rooted_file_parse_count();
    let shared_keys: Vec<_> = related
        .iter()
        .map(|(test, reason)| {
            related_test_rank_key(
                seam,
                &context,
                test,
                *reason,
                owner.as_ref(),
                owner_fn,
                &shared,
            )
        })
        .collect();
    let reused_parses = OwnerPinSyntax::std_rooted_file_parse_count() - before;
    let shared_elapsed = started.elapsed();
    let started = std::time::Instant::now();
    let before = OwnerPinSyntax::std_rooted_file_parse_count();
    // Deliberately retain the previous wrong cache lifetime as an independent
    // control. It must preserve decisions while violating the parse budget.
    let fresh_keys: Vec<_> = related
        .iter()
        .map(|(test, reason)| {
            related_test_rank_key(
                seam,
                &context,
                test,
                *reason,
                owner.as_ref(),
                owner_fn,
                &OwnerPinSyntax::default(),
            )
        })
        .collect();
    let fresh_parses = OwnerPinSyntax::std_rooted_file_parse_count() - before;
    assert!(shared_keys == fresh_keys, "complete ranking keys changed");
    assert_eq!(
        reused_parses, 1,
        "one immutable test file, one cold syntax build"
    );
    assert_eq!(
        fresh_parses,
        related.len(),
        "negative control must reach every cold parse"
    );
    assert!(
        fresh_parses > 1,
        "the one-parse budget rejects the prior lifetime"
    );
    eprintln!(
        "syntax_reuse shared_parses={reused_parses} fresh_parses={fresh_parses} shared={shared_elapsed:?} fresh={:?}",
        started.elapsed()
    );
    let before = OwnerPinSyntax::std_rooted_file_parse_count();
    let evidence = evidence_for_seam_with_context_and_syntax(seam, &context, &shared);
    assert_eq!(evidence.related_tests.len(), 12);
    assert_eq!(
        OwnerPinSyntax::std_rooted_file_parse_count() - before,
        0,
        "ranking and complete evidence must consume the same parsed file"
    );
    Ok(())
}

#[test]
fn syntax_reuse_preserves_complete_evidence_across_order_and_window_boundaries()
-> Result<(), String> {
    let index = syntax_fixture(3, false)?;
    let mut seams = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index);
    assert!(!seams.is_empty());
    let pass = EvidencePass::new(&index);
    let forward = pass.evidence_for(&seams);
    for seam in &seams {
        let single = evidence_for_seam(seam, &index);
        let from_batch = forward
            .iter()
            .find(|e| e.seam_id == *seam.id())
            .ok_or("missing batch seam")?;
        assert_eq!(
            serde_json::to_value(single).map_err(|e| e.to_string())?,
            serde_json::to_value(from_batch).map_err(|e| e.to_string())?
        );
    }
    seams.reverse();
    pass.clear_window_memos();
    let reversed = pass.evidence_for(&seams);
    assert_eq!(
        serde_json::to_value(forward).map_err(|e| e.to_string())?,
        serde_json::to_value(reversed).map_err(|e| e.to_string())?
    );
    Ok(())
}

#[test]
fn equal_paths_in_a_new_index_or_overlay_cannot_reuse_macro_admission() -> Result<(), String> {
    let trusted = syntax_fixture(1, false)?;
    let rebound = syntax_fixture(1, true)?;
    let trusted_seams = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &trusted);
    let rebound_seams = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &rebound);
    let trusted_seam = push_seam(&trusted_seams)?;
    let rebound_seam = push_seam(&rebound_seams)?;
    let trusted_context = CompactGripContext::new(&trusted);
    let rebound_context = CompactGripContext::new(&rebound);
    let stale = OwnerPinSyntax::default();
    let trusted_evidence =
        evidence_for_seam_with_context_and_syntax(trusted_seam, &trusted_context, &stale);
    let fresh = evidence_for_seam_with_context(rebound_seam, &rebound_context);
    assert_eq!(trusted_evidence.discriminate.state, StageState::Yes);
    assert_eq!(
        fresh.discriminate.state,
        StageState::Weak,
        "changed macro binding must change actual matching verdict"
    );
    let wrong = evidence_for_seam_with_context_and_syntax(rebound_seam, &rebound_context, &stale);
    assert_eq!(
        wrong.discriminate.state,
        StageState::Yes,
        "cross-index stale-cache negative control must falsely promote"
    );
    let overlay = |fixture: &FixtureIndex, source: &'static str| -> Result<_, String> {
        assert_eq!(
            fs::read(fixture._fixture_root.join("src/lib.rs")).map_err(|e| e.to_string())?,
            source.as_bytes(),
            "overlay must serve exact indexed source bytes"
        );
        Ok(Arc::new(
            crate::analysis::committed_source::CommittedSourceOverlay::from_entries(
                &fixture._fixture_root,
                [("src/lib.rs", Some(source.as_bytes()))],
            ),
        ))
    };
    let trusted_overlay = overlay(&trusted, TRUSTED_ONE)?;
    let rebound_overlay = overlay(&rebound, SHADOW_ONE)?;
    let trusted_batch =
        crate::analysis::committed_source::with_overlay(Some(trusted_overlay), || {
            evidence_for_seams(&trusted_seams, &trusted)
        });
    assert_eq!(
        trusted_batch
            .iter()
            .find(|e| e.seam_id == *trusted_seam.id())
            .ok_or("missing trusted batch")?
            .discriminate
            .state,
        StageState::Yes
    );
    let actual = crate::analysis::committed_source::with_overlay(Some(rebound_overlay), || {
        evidence_for_seams(&rebound_seams, &rebound)
    });
    let actual = actual
        .iter()
        .find(|e| e.seam_id == *rebound_seam.id())
        .ok_or("batch rebound seam")?;
    assert_eq!(
        serde_json::to_value(&fresh).map_err(|e| e.to_string())?,
        serde_json::to_value(actual).map_err(|e| e.to_string())?
    );
    Ok(())
}
