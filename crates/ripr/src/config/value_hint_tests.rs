//! Near-miss hints for rejected closed-set `ripr.toml` values: a value that is
//! an accepted spelling in another case, padded, or a small typo names the
//! value that was meant. Unrelated values keep the plain accepted-values error.
use super::{parse_config, parse_config_diagnostic};

fn error(text: &str) -> Result<String, String> {
    match parse_config(text) {
        Ok(_) => Err(format!("config must be refused: {text}")),
        Err(message) => Ok(message),
    }
}

#[test]
fn near_miss_values_name_the_accepted_value() -> Result<(), String> {
    for (text, expected) in [
        (
            "[analysis]\nmode = \"Draft\"\n",
            "analysis.mode `Draft` is not supported; expected instant, draft, fast, deep, or ready (did you mean `draft`?)",
        ),
        (
            "[analysis]\nmode = \" deep \"\n",
            "analysis.mode ` deep ` is not supported; expected instant, draft, fast, deep, or ready (did you mean `deep`?)",
        ),
        (
            "[analysis]\nmode = \"redy\"\n",
            "analysis.mode `redy` is not supported; expected instant, draft, fast, deep, or ready (did you mean `ready`?)",
        ),
        (
            "[oracles]\nsnapshot_strength = \"Strong\"\n",
            "oracles.snapshot_strength `Strong` is not supported; expected strong, medium, weak, smoke, none, or unknown (did you mean `strong`?)",
        ),
        (
            "[severity.findings]\nexposed = \"WARNING\"\n",
            "severity.findings.exposed `WARNING` is not supported; expected info, warning, or note (did you mean `warning`?)",
        ),
        (
            "[severity.seams]\nopaque = \"Off\"\n",
            "severity.seams.opaque `Off` is not supported; expected info, warning, or note, or off (did you mean `off`?)",
        ),
    ] {
        assert_eq!(error(text)?, expected, "{text}");
    }
    let language = error("[languages]\nenabled = [\"Rust\"]\n")?;
    assert!(
        language.starts_with("languages.enabled lists unknown language `Rust`; valid values are ")
            && language.ends_with(" (did you mean `rust`?)"),
        "{language}"
    );
    Ok(())
}

#[test]
fn unrelated_values_get_no_hint() -> Result<(), String> {
    for text in [
        "[analysis]\nmode = \"slow\"\n",
        "[analysis]\nmode = \"\"\n",
        "[oracles]\nsnapshot_strength = \"high\"\n",
        "[severity.findings]\nexposed = \"error\"\n",
        "[languages]\nenabled = [\"cobol\"]\n",
    ] {
        let message = error(text)?;
        assert!(!message.contains("did you mean"), "{text}: {message}");
    }
    // `off` is refused for findings with its own recovery, and a findings typo
    // must not be steered to a value that table does not accept.
    let off = error("[severity.findings]\nexposed = \"of\"\n")?;
    assert!(!off.contains("did you mean `off`"), "{off}");
    Ok(())
}

/// The hint rides in the message only: the typed diagnostic keeps the same
/// span, rejected value and accepted-value list.
#[test]
fn hint_does_not_change_the_typed_diagnostic() -> Result<(), String> {
    let diagnostic = match parse_config_diagnostic("[analysis]\nmode = \"Draft\"\n") {
        Ok(_) => return Err("config must be refused".to_string()),
        Err(diagnostic) => diagnostic,
    };
    assert_eq!(diagnostic.kind, "config_validation");
    assert_eq!(diagnostic.invalid_value.as_deref(), Some("\"Draft\""));
    assert_eq!(
        diagnostic.expected_values,
        ["instant", "draft", "fast", "deep", "ready"]
    );
    assert!(diagnostic.message.ends_with("(did you mean `draft`?)"));
    Ok(())
}

/// An oversized rejected value is never aligned against the accepted names:
/// the load still fails with the plain error and no hint. The distance table
/// for a large value would otherwise grow with its length times every name.
#[test]
fn oversized_value_fails_without_a_hint() -> Result<(), String> {
    let huge = "d".repeat(256 * 1024);
    for text in [
        format!("[analysis]\nmode = \"{huge}\"\n"),
        format!("[languages]\nenabled = [\"{huge}\"]\n"),
    ] {
        let message = error(&text)?;
        assert!(message.contains("expected instant") || message.contains("unknown language"));
        assert!(!message.contains("did you mean"));
    }
    assert_eq!(
        super::suggest::did_you_mean(&huge, &["draft", "deep"]),
        None
    );
    Ok(())
}
