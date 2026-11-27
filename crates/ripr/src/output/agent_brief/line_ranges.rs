//! A serialization profile only: never changes selection or drops locations.
use crate::agent::loop_commands::display_path;
use crate::app::agent_brief::AgentBriefLine;
use serde_json::{Value, json};

pub(super) fn render(lines: &[AgentBriefLine]) -> Value {
    let mut groups: Vec<(String, Vec<[usize; 2]>)> = Vec::new();
    for line in lines {
        let file = display_path(&line.file);
        if let Some((previous_file, ranges)) = groups.last_mut()
            && *previous_file == file
        {
            if let Some([_, end]) = ranges.last_mut()
                && end.checked_add(1) == Some(line.line)
            {
                *end = line.line;
            } else {
                ranges.push([line.line, line.line]);
            }
        } else {
            groups.push((file, vec![[line.line, line.line]]));
        }
    }
    Value::Array(
        groups
            .into_iter()
            .map(|(file, ranges)| json!({"file": file, "ranges": ranges}))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouped_ranges_preserve_order_duplicates_reentry_and_boundaries() {
        let lines = [
            ("雪 /a,\".rs", 3),
            ("雪 /a,\".rs", 4),
            ("雪 /a,\".rs", 4),
            ("雪 /a,\".rs", 2),
            ("雪 /a,\".rs", 7),
            ("b.rs", 9),
            ("雪 /a,\".rs", 8),
            ("max.rs", usize::MAX - 1),
            ("max.rs", usize::MAX),
            ("max.rs", 0),
        ]
        .map(|(file, line)| AgentBriefLine::new(file, line));
        assert_eq!(
            render(&lines),
            json!([
                {"file":"雪 /a,\".rs","ranges":[[3,4],[4,4],[2,2],[7,7]]},
                {"file":"b.rs","ranges":[[9,9]]},
                {"file":"雪 /a,\".rs","ranges":[[8,8]]},
                {"file":"max.rs","ranges":[[usize::MAX-1,usize::MAX],[0,0]]}
            ])
        );
        assert_eq!(render(&[]), json!([]));
        assert_eq!(
            render(&[AgentBriefLine::new("a", 1)]),
            json!([{"file":"a","ranges":[[1,1]]}])
        );
    }
}
