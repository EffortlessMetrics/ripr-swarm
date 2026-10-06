//! The generated workflow's PR guidance annotations.
//!
//! `ripr init --ci github` used to emit one `::warning` workflow command
//! per placed review card from an inline jq program. `ripr reports
//! ci-packet` prints the same lines from this renderer. The workflow
//! command is encoded here, never through a TSV or shell round-trip, so a
//! backslash, tab, CR, or LF in a path or message reaches GitHub's decoder
//! as the original bytes (#4089). An annotation names the repair start only
//! when that field is present; the literal string "null" stays absent. The
//! brief command is not interpolated: it points at the runner's checkout.

use super::jq::{Fail, add, at, each_at, index, interpolated, truthy};
use crate::output::workflow_escape::{
    escape_data, escape_property, path_is_unplaceable, unplaced_location_prefix,
};
use serde_json::Value;

/// One `::warning` line per placed comment in a `comments.json` document,
/// each ending in a newline. A value whose type breaks the retired jq
/// program stops the output there, as jq did, and returns the lines
/// printed so far with the error.
pub(crate) fn render_workflow_annotations(document: &Value) -> Result<String, (String, String)> {
    let mut out = String::new();
    let comments = each_at(document, "comments").map_err(|Fail| type_error(&out))?;
    for comment in comments {
        match annotation(comment) {
            Ok(Some(line)) => {
                out.push_str(&line);
                out.push('\n');
            }
            Ok(None) => {}
            Err(Fail) => return Err(type_error(&out)),
        }
    }
    Ok(out)
}

fn type_error(printed: &str) -> (String, String) {
    (
        printed.to_string(),
        "comments.json holds a value whose type the annotation format cannot encode".to_string(),
    )
}

fn annotation(comment: &Value) -> Result<Option<String>, Fail> {
    let path = at(comment, "placement.path")?;
    let line = at(comment, "placement.line")?;
    if !(truthy(path) && truthy(line)) {
        return Ok(None);
    }
    let repair = at(comment, "llm_guidance.repair_command")?;
    let repair = if truthy(repair) {
        repair.clone()
    } else {
        Value::String(String::new())
    };
    let reason = index(comment, "reason")?;
    let reason = if truthy(reason) {
        reason.clone()
    } else {
        Value::String("RIPR targeted test guidance".to_string())
    };
    let suffix = if repair != "" && repair != "null" {
        add(Value::from(" Start the repair: "), repair)?
    } else {
        Value::String(String::new())
    };
    let message = add(reason, suffix)?;
    let path_text = string(path)?;
    let line_text = interpolated(line);
    let message_text = string(&message)?;
    if path_is_unplaceable(path_text) {
        // A property cannot carry a control or bidi character: omit the
        // placement and name the escaped location in the message (#6309).
        let prefix = unplaced_location_prefix(path_text, &line_text);
        return Ok(Some(format!(
            "::warning title=RIPR targeted test guidance::{}",
            escape_data(&format!("{prefix}{message_text}"))
        )));
    }
    let path = escape_property(path_text);
    let line = escape_property(&line_text);
    let message = escape_data(message_text);
    Ok(Some(format!(
        "::warning file={path},line={line},title=RIPR targeted test guidance::{message}"
    )))
}

/// `gsub` takes strings only.
fn string(value: &Value) -> Result<&str, Fail> {
    value.as_str().ok_or(Fail)
}

#[cfg(test)]
mod tests {
    use super::render_workflow_annotations;

    #[test]
    fn control_character_paths_drop_the_placement_and_name_the_location() -> Result<(), String> {
        // #6309: a property cannot carry ESC, so the placed form would name a
        // file that does not exist.
        let document = serde_json::json!({
            "comments": [
                { "placement": { "path": "src/a\u{1b}[2Jb.rs", "line": 7 },
                  "reason": "Pin the value" },
                { "placement": { "path": "src/ok,file.rs", "line": 3 },
                  "reason": "Pin the value" }
            ]
        });
        let rendered = render_workflow_annotations(&document).map_err(|(_, err)| err)?;
        assert_eq!(
            rendered,
            concat!(
                "::warning title=RIPR targeted test guidance::",
                "Location (file name has control characters, so not placed): ",
                "src/a\\u{1b}[2Jb.rs:7. Pin the value\n",
                "::warning file=src/ok%2Cfile.rs,line=3,",
                "title=RIPR targeted test guidance::Pin the value\n"
            )
        );
        Ok(())
    }
}
