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
    use serde_json::{Value, json};

    const TITLE: &str = "title=RIPR targeted test guidance";

    fn rendered(document: &Value) -> Result<String, String> {
        render_workflow_annotations(document).map_err(|(_, err)| err)
    }

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

    #[test]
    fn a_document_without_placed_comments_prints_nothing() -> Result<(), String> {
        // Constructs: each document parses as the shape its case names.
        let no_key = json!({});
        let not_a_list = json!({ "comments": "none" });
        let empty = json!({ "comments": [] });
        let unplaced = json!({ "comments": [
            { "reason": "no placement" },
            { "placement": null, "reason": "null placement" },
            { "placement": { "path": "src/a.rs" }, "reason": "no line" },
            { "placement": { "line": 3 }, "reason": "no path" },
            { "placement": { "path": false, "line": 3 }, "reason": "false path" },
            { "placement": { "path": "src/a.rs", "line": false }, "reason": "false line" }
        ] });
        assert!(no_key.get("comments").is_none());
        assert!(not_a_list["comments"].is_string());
        assert_eq!(empty["comments"].as_array().map(Vec::len), Some(0));
        assert_eq!(unplaced["comments"].as_array().map(Vec::len), Some(6));
        for document in [&no_key, &not_a_list, &empty, &unplaced] {
            assert_eq!(rendered(document)?, "");
        }
        // Discriminating control: the same shape with both fields present places.
        let placed = json!({ "comments": [
            { "placement": { "path": "src/a.rs", "line": 3 }, "reason": "placed" }
        ] });
        assert_eq!(
            rendered(&placed)?,
            format!("::warning file=src/a.rs,line=3,{TITLE}::placed\n")
        );
        Ok(())
    }

    #[test]
    fn a_missing_or_falsy_reason_falls_back_to_the_default_text() -> Result<(), String> {
        let placement = json!({ "path": "src/a.rs", "line": 1 });
        let document = json!({ "comments": [
            { "placement": placement },
            { "placement": placement, "reason": null },
            { "placement": placement, "reason": false },
            { "placement": placement, "reason": "" }
        ] });
        assert_eq!(document["comments"].as_array().map(Vec::len), Some(4));
        let default =
            format!("::warning file=src/a.rs,line=1,{TITLE}::RIPR targeted test guidance\n");
        // An empty string is truthy in jq, so it is kept, not defaulted.
        let kept_empty = format!("::warning file=src/a.rs,line=1,{TITLE}::\n");
        assert_eq!(
            rendered(&document)?,
            format!("{default}{default}{default}{kept_empty}")
        );
        Ok(())
    }

    #[test]
    fn the_repair_start_is_named_only_when_the_field_is_present() -> Result<(), String> {
        let with = |repair: Value| {
            json!({ "comments": [{
                "placement": { "path": "src/a.rs", "line": 2 },
                "reason": "Pin it.",
                "llm_guidance": { "repair_command": repair }
            }] })
        };
        let present = with(json!("ripr agent repair --seam-id s1"));
        assert_eq!(
            present["comments"][0]["llm_guidance"]["repair_command"],
            "ripr agent repair --seam-id s1"
        );
        assert_eq!(
            rendered(&present)?,
            format!(
                "::warning file=src/a.rs,line=2,{TITLE}::Pin it. Start the repair: ripr agent repair --seam-id s1\n"
            )
        );
        // The literal string "null", an empty command and null stay absent.
        let absent = format!("::warning file=src/a.rs,line=2,{TITLE}::Pin it.\n");
        for repair in [json!("null"), json!(""), Value::Null, json!(false)] {
            let document = with(repair);
            assert!(document["comments"][0]["llm_guidance"].is_object());
            assert_eq!(rendered(&document)?, absent);
        }
        // Control: no guidance object at all is also absent.
        let bare = json!({ "comments": [{
            "placement": { "path": "src/a.rs", "line": 2 }, "reason": "Pin it."
        }] });
        assert!(bare["comments"][0].get("llm_guidance").is_none());
        assert_eq!(rendered(&bare)?, absent);
        Ok(())
    }

    #[test]
    fn placement_and_message_text_are_escaped_for_their_slot() -> Result<(), String> {
        let document = json!({ "comments": [{
            "placement": { "path": "a:b,c%d.rs", "line": "7" },
            "reason": "50% done\r\nnext: a,b"
        }] });
        assert_eq!(document["comments"][0]["placement"]["line"], "7");
        // A string line is used raw; property text escapes `:` `,` `%`; data
        // text escapes only `%` and line breaks.
        assert_eq!(
            rendered(&document)?,
            format!("::warning file=a%3Ab%2Cc%25d.rs,line=7,{TITLE}::50%25 done%0D%0Anext: a,b\n")
        );
        // A numeric line prints as compact JSON, and zero is still placed.
        let numeric = json!({ "comments": [{
            "placement": { "path": "a.rs", "line": 0 }, "reason": "r"
        }] });
        assert_eq!(
            rendered(&numeric)?,
            format!("::warning file=a.rs,line=0,{TITLE}::r\n")
        );
        Ok(())
    }

    #[test]
    fn an_unplaceable_path_without_a_line_names_the_file_alone() -> Result<(), String> {
        let document = json!({ "comments": [{
            "placement": { "path": "a\u{7}b.rs", "line": 0 },
            "reason": "r",
            "llm_guidance": { "repair_command": "fix it" }
        }] });
        assert_eq!(document["comments"][0]["placement"]["line"], 0);
        assert_eq!(
            rendered(&document)?,
            format!(
                "::warning {TITLE}::Location (file name has control characters, so not placed): a\\u{{07}}b.rs. r Start the repair: fix it\n"
            )
        );
        Ok(())
    }

    #[test]
    fn a_value_of_the_wrong_type_stops_the_output_with_the_lines_so_far() {
        let good = json!({
            "placement": { "path": "src/a.rs", "line": 1 }, "reason": "first"
        });
        let first_line = format!("::warning file=src/a.rs,line=1,{TITLE}::first\n");
        let message = "comments.json holds a value whose type the annotation format cannot encode";
        let bad_cases = [
            // placement is a string, so `.placement.path` is a type error.
            json!({ "placement": "src/a.rs", "reason": "x" }),
            // a non-string path or reason cannot go through gsub.
            json!({ "placement": { "path": 5, "line": 1 }, "reason": "x" }),
            json!({ "placement": { "path": "a.rs", "line": 1 }, "reason": 5 }),
            // a non-string repair command cannot be appended to the reason.
            json!({
                "placement": { "path": "a.rs", "line": 1 }, "reason": "x",
                "llm_guidance": { "repair_command": 5 }
            }),
            // guidance that is not an object breaks the lookup itself.
            json!({
                "placement": { "path": "a.rs", "line": 1 }, "reason": "x",
                "llm_guidance": "text"
            }),
            // a comment that is not an object breaks `.placement`.
            json!("not an object"),
        ];
        for bad in bad_cases {
            let document = json!({ "comments": [good, bad, good] });
            assert_eq!(document["comments"].as_array().map(Vec::len), Some(3));
            // Control: the good comment alone renders.
            assert_eq!(
                render_workflow_annotations(&json!({ "comments": [good] })),
                Ok(first_line.clone())
            );
            assert_eq!(
                render_workflow_annotations(&document),
                Err((first_line.clone(), message.to_string())),
                "{bad}"
            );
        }
        // A top-level value that is not an object fails before any line.
        assert_eq!(
            render_workflow_annotations(&json!(["x"])),
            Err((String::new(), message.to_string()))
        );
    }
}
