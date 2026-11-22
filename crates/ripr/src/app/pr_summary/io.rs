use super::model::{InputState, JsonInput};
use super::util::first_line;
use serde_json::Value;
use std::path::Path;

pub(super) fn load_json(repo: &Path, relative: &str) -> JsonInput {
    let path = repo.join(relative);
    let text = match crate::bounded_input::read_artifact_text(&path) {
        Ok(text) => text,
        // A file that is there but cannot be decoded (truncated UTF-16 from
        // a PowerShell 5.1 redirect, or not UTF-8 at all) is invalid, not
        // missing, and the reason names the encoding (#15936).
        Err(err) if err.kind() == std::io::ErrorKind::InvalidData => {
            return JsonInput {
                state: InputState::Invalid(first_line(&err.to_string())),
                value: None,
            };
        }
        Err(_) => {
            return JsonInput {
                state: InputState::Missing,
                value: None,
            };
        }
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(value) => JsonInput {
            state: InputState::Present,
            value: Some(value),
        },
        Err(err) => JsonInput {
            state: InputState::Invalid(first_line(&err.to_string())),
            value: None,
        },
    }
}

pub(super) fn file_state(repo: &Path, relative: &str) -> &'static str {
    if repo.join(relative).exists() {
        "present"
    } else {
        "missing"
    }
}
