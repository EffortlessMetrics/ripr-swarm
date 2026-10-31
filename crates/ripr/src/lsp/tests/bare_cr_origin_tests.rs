use super::*;
use tower_lsp_server::ls_types::Uri;

#[test]
fn rust_analysis_deleted_eof_origins_keep_the_terminal_editor_row() -> Result<(), String> {
    use crate::analysis::diagnostic_origin::OriginKind;
    use crate::domain::SourceCurrentness;

    for ending in ["\n", "\r\n"] {
        let fixture = unique_lsp_test_root("deleted-eof-origin")?;
        let root = fixture.path();
        fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"deleted_eof_origin\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .map_err(|error| error.to_string())?;
        run_lsp_scope_git(root, &["init", "-q"])?;
        let path = root.join("src/lib.rs");
        let after = format!("pub fn a() {{}}\r// note{ending}");
        let before = format!(
            "{after}pub fn removed(amount: i32, threshold: i32) -> bool {{ amount > threshold }}{ending}"
        );
        fs::write(&path, &before).map_err(|error| error.to_string())?;
        run_lsp_scope_git(root, &["add", "Cargo.toml", "src/lib.rs"])?;
        run_lsp_scope_git(
            root,
            &[
                "-c",
                "user.email=ripr-test@example.com",
                "-c",
                "user.name=ripr-test",
                "-c",
                "commit.gpgSign=false",
                "commit",
                "-qm",
                "deleted EOF baseline",
            ],
        )?;
        fs::write(&path, &after).map_err(|error| error.to_string())?;
        // The worktree loader uses zero context and records this deletion at
        // analyzer row 1. The supported diff-file path retains context, whose
        // preceding row advances the deletion's new-side coordinate to EOF.
        let diff = lsp_scope_git_output(
            root,
            &[
                "diff",
                "--no-ext-diff",
                "--no-color",
                "--unified=3",
                "HEAD",
                "--",
                "src/lib.rs",
            ],
        )?;
        assert!(diff.status.success(), "create context-bearing EOF diff");
        assert!(String::from_utf8_lossy(&diff.stdout).contains("@@ -1,2 +1 @@"));
        let diff_path = root.join("deleted-eof.diff");
        fs::write(&diff_path, &diff.stdout).map_err(|error| error.to_string())?;
        let output = crate::analysis::run_analysis(&crate::analysis::AnalysisOptions {
            root: root.to_path_buf(),
            base: None,
            diff_file: Some(diff_path),
            mode: crate::analysis::AnalysisMode::Draft,
            include_unchanged_tests: false,
            resolve_tsconfig_paths: false,
            perl_facts_path: None,
            perl_producer_failure: None,
            git_timeout: None,
            git_candidate: None,
            production_like_targets: Default::default(),
            test_harnesses: Vec::new(),
            resolved_subject_identity: None,
            open_rust_index_paths: Default::default(),
        })?;
        let origins = &output.rust_diagnostic_origins;
        let deleted = output
            .findings
            .iter()
            .filter(|finding| {
                finding.source_currentness == SourceCurrentness::BaseDeleted
                    && finding.probe.location.file.ends_with("src/lib.rs")
            })
            .collect::<Vec<_>>();
        assert!(
            !deleted.is_empty(),
            "analysis must produce removed-only EOF evidence"
        );
        for finding in deleted {
            assert_eq!(
                finding.probe.location.line, 2,
                "removed-only new-side analyzer row"
            );
            let origin = origins
                .for_finding(finding)
                .ok_or("deleted finding must retain a Rust origin")?;
            assert_eq!(origin.kind, OriginKind::CoarseZeroWidth);
            let grouped = finding_diagnostics_by_uri_with_profile(
                root,
                std::slice::from_ref(finding),
                &crate::config::SeverityConfig::default(),
                true,
                FindingDiagnosticProjection::new(
                    crate::config::LspDiagnosticProfile::Full,
                    &PositionEncodingKind::UTF16,
                    origins,
                ),
            )?;
            let diagnostic = grouped
                .values()
                .flatten()
                .next()
                .ok_or("deleted finding must project in Full profile")?;
            assert_eq!(diagnostic.range.start, Position::new(2, 0));
            assert_eq!(diagnostic.range.end, Position::new(2, 120));
        }
    }
    Ok(())
}

#[test]
fn rust_analysis_origins_project_editor_rows_for_bare_cr() -> Result<(), String> {
    use crate::analysis::diagnostic_origin::OriginKind;

    for (name, breaks, coarse) in [
        ("cr", ["\r", "\r", "\r", "\r"], true),
        ("mixed", ["\n", "\r", "\r", "\n"], true),
        ("earlier-cr", ["\r", "\n", "\n", "\n"], true),
        ("lf", ["\n", "\n", "\n", "\n"], false),
        ("crlf", ["\r\n", "\r\n", "\r\n", "\r\n"], false),
    ] {
        for (variant, lines, old_predicate, predicate, columns) in [
            (
                "ascii",
                [
                    "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {",
                    "    let marker = 10;",
                    "    if amount > discount_threshold { amount - marker } else { amount }",
                    "}",
                ],
                "amount > discount_threshold",
                "amount >= discount_threshold",
                [(7, 35), (7, 35), (7, 35)],
            ),
            (
                "unicode-decoy",
                [
                    "pub fn discounted_total(montant_é: i32, discount_threshold: i32) -> i32 {",
                    "    let marker = 10; let _ = \"🎉 montant_é >= discount_threshold\";",
                    "    /* 🎉 */ if montant_é > discount_threshold { montant_é - marker } else { montant_é }",
                    "}",
                ],
                "montant_é > discount_threshold",
                "montant_é >= discount_threshold",
                [(18, 50), (16, 47), (15, 46)],
            ),
        ] {
            let name = format!("{name}-{variant}");
            let fixture = boundary_gap_git_fixture_root(&format!("origin-row-{name}"))?;
            let root = fixture.path();
            let path = root.join("src/lib.rs");
            let before = lines
                .into_iter()
                .zip(breaks)
                .map(|(line, ending)| format!("{line}{ending}"))
                .collect::<String>();
            fs::write(&path, &before).map_err(|error| format!("{name} baseline: {error}"))?;
            run_lsp_scope_git(root, &["add", "src/lib.rs"])?;
            run_lsp_scope_git(
                root,
                &[
                    "-c",
                    "user.email=ripr-test@example.com",
                    "-c",
                    "user.name=ripr-test",
                    "-c",
                    "commit.gpgSign=false",
                    "commit",
                    "-qm",
                    "line-ending baseline",
                ],
            )?;
            // The Unicode decoy already contains the new predicate. Only the
            // executable predicate changes, so first-substring search is wrong.
            let after = before.replace(old_predicate, predicate);
            assert_ne!(before, after, "{name}: setup must change a predicate");
            fs::write(&path, &after).map_err(|error| format!("{name} edit: {error}"))?;
            let config = boundary_gap_lsp_config(crate::config::RiprConfig::default());
            let (output, origins, consumed, _completeness) =
                crate::app::check_workspace_worktree_with_sources_open_rust_paths_and_progress(
                    config.check_input(root),
                    config.repo_config(),
                    &BTreeSet::new(),
                    None,
                )?;
            assert_eq!(
                consumed.digest(Path::new("src/lib.rs")),
                Some(content_digest(after.as_bytes()))
            );
            let finding = output
                .findings
                .iter()
                .find(|finding| finding.probe.expression == predicate)
                .ok_or_else(|| {
                    format!(
                        "{name}: analysis must produce the changed Rust predicate: {:?}",
                        output.findings
                    )
                })?;
            let origin = origins
                .for_finding(finding)
                .ok_or_else(|| format!("{name}: Rust analysis must retain a real origin"))?;
            assert_eq!(
                origin.kind,
                if coarse {
                    OriginKind::CoarseZeroWidth
                } else {
                    OriginKind::Exact
                },
                "{name}"
            );

            // Projection must consume the captured origin even after disk moves on.
            fs::write(&path, "// later saved contents\n").map_err(|error| error.to_string())?;
            for (encoding, exact_columns) in [
                PositionEncodingKind::UTF8,
                PositionEncodingKind::UTF16,
                PositionEncodingKind::UTF32,
            ]
            .into_iter()
            .zip(columns)
            {
                let grouped = finding_diagnostics_by_uri_with_profile(
                    root,
                    std::slice::from_ref(finding),
                    &crate::config::SeverityConfig::default(),
                    true,
                    FindingDiagnosticProjection::new(
                        crate::config::LspDiagnosticProfile::Full,
                        &encoding,
                        &origins,
                    ),
                )?;
                let diagnostic =
                    grouped.values().flatten().next().ok_or_else(|| {
                        format!("{name}: expected a published finding diagnostic")
                    })?;
                assert_eq!(diagnostic.range.start.line, 2, "{name} {encoding:?}");
                assert_eq!(diagnostic.range.end.line, 2, "{name} {encoding:?}");
                let expected_columns = if coarse { (0, 120) } else { exact_columns };
                assert_eq!(
                    (
                        diagnostic.range.start.character,
                        diagnostic.range.end.character
                    ),
                    expected_columns,
                    "{name} {encoding:?}"
                );
                if coarse {
                    assert_eq!(origin.utf8.start, origin.utf8.end, "{name}");
                    assert_eq!(origin.utf16.start, origin.utf16.end, "{name}");
                    assert_eq!(origin.utf32.start, origin.utf32.end, "{name}");
                }
            }
        }
    }
    Ok(())
}

/// Production fixture shared by the lens/hover row tests: a four-line Rust
/// file whose changed predicate is on editor row 2, analyzed through the real
/// saved-workspace path into a committed-shape snapshot.
fn lens_row_snapshot(
    name: &str,
    breaks: [&str; 4],
) -> Result<(TempLspRoot, WorkspaceDiagnostics, Uri), String> {
    let fixture = boundary_gap_git_fixture_root(&format!("lens-row-{name}"))?;
    let root = fixture.path();
    let path = root.join("src/lib.rs");
    let lines = [
        "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {",
        "    let marker = 10;",
        "    if amount > discount_threshold { amount - marker } else { amount }",
        "}",
    ];
    let before = lines
        .into_iter()
        .zip(breaks)
        .map(|(line, ending)| format!("{line}{ending}"))
        .collect::<String>();
    fs::write(&path, &before).map_err(|error| format!("{name} baseline: {error}"))?;
    run_lsp_scope_git(root, &["add", "src/lib.rs"])?;
    run_lsp_scope_git(
        root,
        &[
            "-c",
            "user.email=ripr-test@example.com",
            "-c",
            "user.name=ripr-test",
            "-c",
            "commit.gpgSign=false",
            "commit",
            "-qm",
            "line-ending baseline",
        ],
    )?;
    let after = before.replace(
        "amount > discount_threshold",
        "amount >= discount_threshold",
    );
    assert_ne!(before, after, "{name}: setup must change the predicate");
    fs::write(&path, &after).map_err(|error| format!("{name} edit: {error}"))?;
    let repo_config = crate::config::RiprConfig::default();
    let config = boundary_gap_lsp_config(repo_config);
    let diagnostics = workspace_diagnostics_with_config(root, &config, false)?;
    assert!(
        diagnostics
            .snapshot
            .findings
            .iter()
            .any(|finding| finding.probe.expression == "amount >= discount_threshold"),
        "{name}: analysis must produce the changed predicate"
    );
    let uri = file_uri_for_path(&path)?;
    Ok((fixture, diagnostics, uri))
}

const LENS_LAYOUTS: [(&str, [&str; 4]); 4] = [
    ("cr", ["\r", "\r", "\r", "\r"]),
    ("mixed", ["\n", "\r", "\r", "\n"]),
    ("lf", ["\n", "\n", "\n", "\n"]),
    ("crlf", ["\r\n", "\r\n", "\r\n", "\r\n"]),
];

fn lens_rows(uri: &Uri, snapshot: &AnalysisSnapshot) -> Vec<u32> {
    code_lens_response(uri, Some(snapshot))
        .iter()
        .map(|lens| lens.range.start.line)
        .collect()
}

#[test]
fn analysis_snapshot_lens_rows_use_captured_editor_row() -> Result<(), String> {
    for (name, breaks) in LENS_LAYOUTS {
        let (fixture, diagnostics, uri) = lens_row_snapshot(name, breaks)?;
        let snapshot = &diagnostics.snapshot;
        assert!(
            !snapshot.rust_diagnostic_origins.is_empty(),
            "{name}: snapshot must retain the captured origins"
        );
        let rows = lens_rows(&uri, snapshot);
        assert!(!rows.is_empty(), "{name}: expected a lens");
        assert!(rows.iter().all(|row| *row == 2), "{name}: lens {rows:?}");

        // The semantic refresh identity uses the same owner as placement.
        let identity = lens_view_identity(snapshot);
        let identity_rows = identity.rows();
        assert!(!identity_rows.is_empty(), "{name}");
        assert!(
            identity_rows.iter().all(|row| *row == 2),
            "{name}: identity {identity_rows:?}"
        );

        // A later save cannot move a captured snapshot's lens or identity.
        fs::write(
            fixture.path().join("src/lib.rs"),
            "// later saved contents\n\n\n\n",
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(lens_rows(&uri, snapshot), rows, "{name}: disk move");
        assert_eq!(lens_view_identity(snapshot), identity, "{name}: disk move");

        // Alternate control: without origins the explicit conservative
        // fallback is the analyzer row, which differs only where a bare CR
        // makes the analyzer and editor models disagree.
        let mut fallback = snapshot.clone();
        fallback.rust_diagnostic_origins = Default::default();
        let fallback_rows = lens_rows(&uri, &fallback);
        assert!(!fallback_rows.is_empty(), "{name}: fallback lens");
        let expected_fallback = match name {
            "cr" => 0,
            "mixed" => 1,
            _ => 2,
        };
        assert!(
            fallback_rows.iter().all(|row| *row == expected_fallback),
            "{name}: fallback {fallback_rows:?}"
        );
        let fallback_identity = lens_view_identity(&fallback);
        assert_eq!(
            fallback_identity == identity,
            expected_fallback == 2,
            "{name}: identity must follow the row"
        );
    }
    Ok(())
}

#[test]
fn analysis_snapshot_published_hover_matches_captured_row() -> Result<(), String> {
    for (name, breaks) in LENS_LAYOUTS {
        let (_fixture, diagnostics, uri) = lens_row_snapshot(name, breaks)?;
        let (service, _socket) = LspService::new(|client| Backend::new(client, PathBuf::from(".")));
        let backend = service.inner();
        backend
            .refresh_plan(diagnostics)
            .ok_or_else(|| format!("{name}: expected refresh plan"))?;
        let hover = backend
            .hover_for_position(&hover_params(uri, 2, 0))
            .ok_or_else(|| format!("{name}: expected hover on editor row 2"))?;
        let HoverContents::Markup(markup) = hover.contents else {
            return Err(format!("{name}: expected markup hover"));
        };
        assert!(
            markup.value.contains("amount >= discount_threshold"),
            "{name}: {}",
            markup.value
        );
    }
    Ok(())
}

#[test]
fn analysis_snapshot_unpublished_line_lookup_uses_captured_row() -> Result<(), String> {
    use crate::lsp::lens::lens_findings_at_line;
    // Line-hover lookup for an unpublished finding reads only the snapshot's
    // findings and captured origins, never the published diagnostics.
    for (name, breaks) in LENS_LAYOUTS {
        let (_fixture, mut diagnostics, uri) = lens_row_snapshot(name, breaks)?;
        diagnostics.snapshot.diagnostics_by_uri.clear();
        let snapshot = &diagnostics.snapshot;
        let on_row = lens_findings_at_line(&uri, snapshot, 2);
        assert!(
            on_row
                .iter()
                .any(|finding| finding.probe.expression == "amount >= discount_threshold"),
            "{name}: lookup on editor row 2"
        );
        for row in [0u32, 1] {
            assert!(
                lens_findings_at_line(&uri, snapshot, row).is_empty(),
                "{name}: analyzer row {row} must not select the finding"
            );
        }

        // Alternate control: no origins selects the analyzer-row fallback.
        let mut fallback = snapshot.clone();
        fallback.rust_diagnostic_origins = Default::default();
        let fallback_row = match name {
            "cr" => 0,
            "mixed" => 1,
            _ => 2,
        };
        assert!(
            !lens_findings_at_line(&uri, &fallback, fallback_row).is_empty(),
            "{name}: fallback row {fallback_row}"
        );
    }
    Ok(())
}

#[test]
fn analysis_snapshot_lens_excludes_deleted_findings_with_captured_origins() -> Result<(), String> {
    let (_fixture, mut diagnostics, uri) = lens_row_snapshot("cr", LENS_LAYOUTS[0].1)?;
    for finding in &mut diagnostics.snapshot.findings {
        finding.source_currentness = crate::domain::SourceCurrentness::BaseDeleted;
    }
    assert!(!diagnostics.snapshot.rust_diagnostic_origins.is_empty());
    assert!(lens_rows(&uri, &diagnostics.snapshot).is_empty());
    assert!(lens_view_identity(&diagnostics.snapshot).rows().is_empty());
    Ok(())
}
