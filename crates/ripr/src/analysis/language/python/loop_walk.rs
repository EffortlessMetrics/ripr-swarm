//! Deliberately closed generator walk for rebound while parameters (#9733).
//!
//! Read only a sole `while name OP integer` with one plain yield and one
//! unconditional unit step. Other statements, nesting or rebinding retain the
//! existing unresolved limitation; this is not a general Python evaluator.
use super::{PythonOwner, guard_activation::leading_whitespace};

pub(super) struct LoopWalk {
    pub(super) local: String,
    pub(super) bound: String,
    decreasing: bool,
}

impl LoopWalk {
    pub(super) fn reaches(&self, start: i128) -> bool {
        let Ok(bound) = self.bound.parse::<i128>() else {
            return false;
        };
        if self.decreasing {
            start >= bound
        } else {
            start <= bound
        }
    }
}

pub(super) fn generator_walk(
    source: &str,
    owner: &PythonOwner,
    new_line: &str,
    old_line: Option<&str>,
    local: &str,
    bound: &str,
) -> Option<LoopWalk> {
    bound.parse::<i128>().ok()?;
    if !owner.parameters.iter().any(|p| p.name == local) {
        return None;
    }
    let lines: Vec<_> = source
        .lines()
        .skip(owner.start_line.checked_sub(1)?)
        .take(owner.end_line.checked_sub(owner.start_line)? + 1)
        .filter_map(|raw| {
            let code = raw.split('#').next()?.trim_end();
            (!code.trim().is_empty()).then_some(code)
        })
        .collect();
    let [definition, header, yielded, step] = lines.as_slice() else {
        return None;
    };
    if !definition.trim_start().starts_with("def ")
        || header.trim() != new_line.trim()
        || leading_whitespace(header) <= leading_whitespace(definition)
        || leading_whitespace(yielded) <= leading_whitespace(header)
        || leading_whitespace(step) != leading_whitespace(yielded)
        || yielded.trim() != format!("yield {local}")
    {
        return None;
    }
    for (new_op, old_op, decreasing) in [
        (">=", ">", true),
        (">", ">=", true),
        ("<=", "<", false),
        ("<", "<=", false),
    ] {
        if header.trim() == format!("while {local} {new_op} {bound}:")
            && old_line?.trim() == format!("while {local} {old_op} {bound}:")
            && step.trim() == format!("{local} {}= 1", if decreasing { "-" } else { "+" })
        {
            return Some(LoopWalk {
                local: local.to_string(),
                bound: bound.to_string(),
                decreasing,
            });
        }
    }
    None
}
