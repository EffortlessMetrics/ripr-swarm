use super::*;

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
        let fixture = boundary_gap_git_fixture_root(&format!("origin-row-{name}"))?;
        let root = fixture.path();
        let path = root.join("src/lib.rs");
        let before = [
            "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {",
            "    let marker = 10;",
            "    if amount > discount_threshold { amount - marker } else { amount }",
            "}",
        ]
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
            .find(|finding| finding.probe.expression == "amount >= discount_threshold")
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
        for encoding in [
            PositionEncodingKind::UTF8,
            PositionEncodingKind::UTF16,
            PositionEncodingKind::UTF32,
        ] {
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
            let diagnostic = grouped
                .values()
                .flatten()
                .next()
                .ok_or_else(|| format!("{name}: expected a published finding diagnostic"))?;
            assert_eq!(diagnostic.range.start.line, 2, "{name} {encoding:?}");
            assert_eq!(diagnostic.range.end.line, 2, "{name} {encoding:?}");
            let expected_columns = if coarse { (0, 120) } else { (7, 35) };
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
    Ok(())
}
