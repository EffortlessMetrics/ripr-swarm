pub(crate) fn check_local_context() -> Result<(), String> {
    crate::check_local_context_impl()
}

#[cfg(test)]
mod tests {
    use crate::tests::temp_dir;
    use crate::{
        forbidden_local_context_allowlist_pattern, local_context_findings_for_path,
        local_context_line_summary, read_local_context_allowlist,
    };
    use std::fs;
    use std::path::Path;

    fn write(dir: &Path, name: &str, text: &str) -> Result<String, String> {
        let path = dir.join(name);
        fs::write(&path, text).map_err(|error| format!("write {path:?}: {error}"))?;
        // Forward slashes, as `tracked_files` hands paths to the gate, so the
        // file-name split behaves the same on Windows.
        Ok(path.to_string_lossy().replace('\\', "/"))
    }

    /// Comments and blank lines are skipped, fields are trimmed, backslash
    /// paths are normalized, and each entry keeps its 1-based file line.
    #[test]
    fn allowlist_reader_keeps_rows_with_their_file_lines() -> Result<(), String> {
        let dir = temp_dir("local-context-allowlist");
        let path = write(
            &dir,
            "allow.txt",
            "# path|pattern|max_count|reason\n   \ndocs/a.md | pat-one |2| example reason\ndocs\\b.md|pat-two|0|reason two\n",
        )?;
        let rows = read_local_context_allowlist(&path)?;
        assert_eq!(
            rows.iter()
                .map(|row| (
                    row.path.as_str(),
                    row.pattern.as_str(),
                    row.max_count,
                    row.line
                ))
                .collect::<Vec<_>>(),
            vec![
                ("docs/a.md", "pat-one", 2, 3),
                ("docs/b.md", "pat-two", 0, 4)
            ]
        );
        Ok(())
    }

    /// Each malformed row is refused with its own reason and file line: a
    /// wrong field count, any one of path, pattern or reason blank, and a
    /// max_count that is not a number.
    #[test]
    fn allowlist_reader_refuses_each_malformed_row_by_line() -> Result<(), String> {
        let dir = temp_dir("local-context-allowlist-refusals");
        let cases = [
            ("# c\na|p|1\n", "2 expected path|pattern|max_count|reason"),
            (
                "a|p|1|r|extra\n",
                "1 expected path|pattern|max_count|reason",
            ),
            (
                " |p|1|r\n",
                "1 allowlist entries require path, pattern, and reason",
            ),
            (
                "a| |1|r\n",
                "1 allowlist entries require path, pattern, and reason",
            ),
            (
                "a|p|1| \n",
                "1 allowlist entries require path, pattern, and reason",
            ),
            ("a|p|x|r\n", "1 invalid max_count: "),
        ];
        for (index, (text, expected)) in cases.iter().enumerate() {
            let path = write(&dir, &format!("allow-{index}.txt"), text)?;
            let error = match read_local_context_allowlist(&path) {
                Ok(rows) => return Err(format!("{text:?} was accepted: {} rows", rows.len())),
                Err(error) => error,
            };
            assert!(
                error.starts_with(&format!("{path}:{expected}")),
                "{text:?}: {error}"
            );
        }
        Ok(())
    }

    /// Every real-state marker is refused on its own: the fixed markers in
    /// any letter case (the fixtures use upper case for two of them), drive
    /// paths, and the two home-directory prefixes as spelled. A generic
    /// placeholder path and ordinary text are allowed.
    #[test]
    fn allowlist_pattern_refuses_each_real_state_marker_and_allows_generic_text() {
        for pattern in [
            concat!("see ", ".", "CODEX", "/notes"),
            concat!("MEMORY", ".md"),
            concat!("sandbox:", "/mnt", "/data", "/x"),
            concat!("/mnt", "/data", "/x"),
            concat!("contentreference", "[oaicite:1]"),
            concat!("D:", "\\work\\repo"),
            concat!("/home", "/someone/repo"),
            concat!("/Users", "/someone/repo"),
        ] {
            assert!(
                forbidden_local_context_allowlist_pattern(pattern),
                "{pattern} must be refused"
            );
        }
        for pattern in [
            "Recorded on <date>",
            "plain text",
            concat!("C:", "\\path\\to\\repo"),
            concat!("C:", "/path/to/repo"),
        ] {
            assert!(
                !forbidden_local_context_allowlist_pattern(pattern),
                "{pattern} must be allowed"
            );
        }
    }

    /// A scanned file reports each finding on its 1-based line; a file type
    /// outside the scan is not read; a runtime-state file name is reported
    /// with no line.
    #[test]
    fn path_findings_name_lines_skip_unscanned_types_and_flag_runtime_names() -> Result<(), String>
    {
        let dir = temp_dir("local-context-findings");
        let text = format!(
            "first line\nfrom `{}`\n",
            concat!("H:", "\\Code\\Rust\\ripr")
        );
        let scanned = write(&dir, "notes.md", &text)?;
        let findings = local_context_findings_for_path(&scanned)?;
        assert_eq!(
            findings
                .iter()
                .map(|finding| (finding.line, finding.pattern.as_str()))
                .collect::<Vec<_>>(),
            vec![(Some(2), concat!("H:", "\\Code\\Rust\\ripr"))]
        );

        let unscanned = write(&dir, "notes.png", &text)?;
        assert_eq!(local_context_findings_for_path(&unscanned)?, Vec::new());
        // An unscanned type is not opened at all: a missing `.png` is not a
        // read error.
        let missing = format!("{}/missing.png", dir.to_string_lossy().replace('\\', "/"));
        assert_eq!(local_context_findings_for_path(&missing)?, Vec::new());

        let runtime = write(&dir, concat!("CURRENT", "_STATE.md"), "clean text\n")?;
        let findings = local_context_findings_for_path(&runtime)?;
        assert_eq!(
            findings
                .iter()
                .map(|finding| (
                    finding.line,
                    finding.pattern.as_str(),
                    finding.problem.as_str()
                ))
                .collect::<Vec<_>>(),
            vec![(
                None,
                concat!("CURRENT", "_STATE.md"),
                "committed runtime/session artifact filename"
            )]
        );
        Ok(())
    }

    /// Line summaries are sorted and deduplicated; a finding with no line
    /// is summarized as the file name.
    #[test]
    fn line_summary_sorts_dedups_and_names_file_only_findings() {
        assert_eq!(
            local_context_line_summary(&[Some(5), None, Some(2), Some(5)]),
            "2, 5"
        );
        assert_eq!(local_context_line_summary(&[None]), "file name");
    }
}
