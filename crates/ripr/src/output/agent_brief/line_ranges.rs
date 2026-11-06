//! RIPR-SPEC-0010: lossless projection of ordered, displayed line records.

use crate::agent::loop_commands::display_path;
use crate::app::agent_brief::AgentBriefLine;
use serde_json::{Value, json};

struct FileLineRanges {
    file: String,
    ranges: Vec<(usize, usize)>,
}

pub(super) fn changed_line_ranges_json(lines: &[AgentBriefLine]) -> Value {
    let mut groups: Vec<FileLineRanges> = Vec::new();
    for line in lines {
        let file = display_path(&line.file);
        if let Some(group) = groups.last_mut()
            && group.file == file
        {
            if let Some((_, end)) = group.ranges.last_mut()
                && end.checked_add(1) == Some(line.line)
            {
                *end = line.line;
            } else {
                group.ranges.push((line.line, line.line));
            }
        } else {
            groups.push(FileLineRanges {
                file,
                ranges: vec![(line.line, line.line)],
            });
        }
    }
    json!(groups.into_iter().map(|group| json!({
        "file": group.file,
        "ranges": group.ranges,
    })).collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::*;

    // The oracle expands inclusive intervals in their emitted order. It does
    // not call the range builder or normalize/sort/deduplicate the records.
    fn assert_projection(input: &[(&str, usize)], expected: Value) -> Result<(), String> {
        let lines = input
            .iter()
            .map(|(file, line)| AgentBriefLine::new(*file, *line))
            .collect::<Vec<_>>();
        let actual = changed_line_ranges_json(&lines);
        assert_eq!(actual, expected);
        let mut expanded = Vec::new();
        for group in actual.as_array().ok_or("groups must be an array")? {
            let file = group["file"].as_str().ok_or("group file must be text")?;
            for pair in group["ranges"].as_array().ok_or("ranges must be an array")? {
                let [start, end]: [u64; 2] =
                    serde_json::from_value(pair.clone()).map_err(|error| error.to_string())?;
                assert!(start <= end);
                for line in start..=end {
                    expanded.push(json!({"file": file, "line": line}));
                }
            }
        }
        let original = lines
            .iter()
            .map(|line| json!({"file": display_path(&line.file), "line": line.line}))
            .collect::<Vec<_>>();
        assert_eq!(expanded, original);
        Ok(())
    }

    #[test]
    fn line_ranges_preserve_order_duplicates_and_file_reentry() -> Result<(), String> {
        assert_projection(
            &[
                ("src/a.rs", 3),
                ("src/a.rs", 4),
                ("src/a.rs", 4),
                ("src/a.rs", 5),
                ("src/a.rs", 2),
                ("src/b.rs", 3),
                ("src/b.rs", 4),
                ("src/a.rs", 5),
                ("src/a.rs", 6),
                ("src/a.rs", 9),
            ],
            json!([
                {"file": "src/a.rs", "ranges": [[3, 4], [4, 5], [2, 2]]},
                {"file": "src/b.rs", "ranges": [[3, 4]]},
                {"file": "src/a.rs", "ranges": [[5, 6], [9, 9]]},
            ]),
        )
    }

    #[test]
    fn line_ranges_preserve_empty_singleton_and_displayed_paths() -> Result<(), String> {
        assert_projection(&[], json!([]))?;
        assert_projection(
            &[("src/λ space [x] #%.rs", 8)],
            json!([{"file": "src/λ space [x] #%.rs", "ranges": [[8, 8]]}]),
        )?;
        assert_projection(
            &[("src\\a.rs", 1), ("src/a.rs", 2)],
            json!([{"file": "src/a.rs", "ranges": [[1, 2]]}]),
        )
    }

    #[test]
    fn line_ranges_do_not_wrap_at_usize_max() -> Result<(), String> {
        assert_projection(
            &[
                ("a.rs", usize::MAX - 1),
                ("a.rs", usize::MAX),
                ("a.rs", 0),
                ("a.rs", 1),
                ("a.rs", usize::MAX),
                ("a.rs", usize::MAX),
            ],
            json!([{
                "file": "a.rs",
                "ranges": [
                    [usize::MAX - 1, usize::MAX],
                    [0, 1],
                    [usize::MAX, usize::MAX],
                    [usize::MAX, usize::MAX],
                ],
            }]),
        )
    }
}
