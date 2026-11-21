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
            "[lsp]\ndiagnostic_profile = \"Full\"\n",
            "invalid lsp.diagnostic_profile \"Full\"; expected \"actionable\" or \"full\" in [lsp] (did you mean `full`?)",
        ),
        (
            "[lsp]\ndiagnostic_profile = \"actionabel\"\n",
            "invalid lsp.diagnostic_profile \"actionabel\"; expected \"actionable\" or \"full\" in [lsp] (did you mean `actionable`?)",
        ),
        (
            "[perl]\nproducer = \"perl_ripr_facts\"\n",
            "perl.producer `perl_ripr_facts` is not supported; expected perl-ripr-facts, perllsp, or perl-lsp (did you mean `perl-ripr-facts`?)",
        ),
        (
            "[severity.seams]\nopaque = \"Off\"\n",
            "severity.seams.opaque `Off` is not supported; expected info, warning, note, or off (did you mean `off`?)",
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
        "[lsp]\ndiagnostic_profile = \"verbose\"\n",
        "[perl]\nproducer = \"python\"\n",
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

/// #15898: a value of the wrong TOML shape is described in TOML terms, with
/// one fixed line on how to write it. The deserializer's own vocabulary
/// (`usize`, `a sequence`, an internal `struct` name) never reaches the user.
#[test]
fn wrong_shape_values_name_the_toml_shape_and_how_to_write_it() -> Result<(), String> {
    for (text, reason, next) in [
        (
            "[reports]\nmax_related_tests = -1\n",
            "invalid value: integer `-1`, expected a whole number of 0 or more",
            "write a whole number of 0 or more with no quotes, for example `5`",
        ),
        (
            "[reports]\nmax_related_tests = \"five\"\n",
            "invalid type: string \"five\", expected a whole number of 0 or more",
            "write a whole number of 0 or more with no quotes, for example `5`",
        ),
        (
            "[lsp]\nseam_diagnostics = \"yes\"\n",
            "invalid type: string \"yes\", expected true or false",
            "write `true` or `false` with no quotes",
        ),
        (
            "[analysis]\nmode = 5\n",
            "invalid type: integer `5`, expected a quoted string",
            "write the value in double quotes, for example `\"value\"`",
        ),
        (
            "[languages]\nenabled = \"rust\"\n",
            "invalid type: string \"rust\", expected a list",
            "write a list in square brackets, for example `[\"value\"]`",
        ),
        (
            "[overrides]\npath = \"src\"\n",
            "invalid type: map, expected a list of tables",
            "write each entry under a double-bracket header, for example `[[overrides]]`",
        ),
        (
            "analysis = 5\n",
            "invalid type: integer `5`, expected a table",
            "write it as a table header on its own line, for example `[analysis]`, with its keys on the lines below",
        ),
    ] {
        let message = error(text)?;
        let lines: Vec<&str> = message.lines().collect();
        // The reason is the parser's last line; the next step follows a
        // blank line, like the other hints.
        assert_eq!(
            lines
                .iter()
                .rev()
                .take(3)
                .rev()
                .copied()
                .collect::<Vec<_>>(),
            [reason, "", next],
            "{text}: {message}"
        );
        for jargon in ["usize", "a sequence", "a boolean", "struct ", "Raw"] {
            assert!(!message.contains(jargon), "{text}: {jargon} in {message}");
        }
        // The source-free summary carries the next step and no file text.
        let summary = super::config_error_summary(&message);
        assert!(summary.ends_with(&format!("; {next}")), "{summary}");
        assert!(!summary.contains('\n'), "{summary}");
    }
    Ok(())
}

/// The shape is read from the end of the parser's reason, which is the
/// parser's own text. A value that quotes another expectation does not pick
/// the hint, and is echoed unchanged.
#[test]
fn a_value_quoting_an_expectation_does_not_choose_the_hint() -> Result<(), String> {
    let message = error("[lsp]\nseam_diagnostics = \"1, expected usize\"\n")?;
    assert!(
        message.contains("invalid type: string \"1, expected usize\", expected true or false"),
        "{message}"
    );
    assert!(
        message.ends_with("\nwrite `true` or `false` with no quotes"),
        "{message}"
    );
    assert!(!message.contains("whole number"), "{message}");
    Ok(())
}

/// Errors that are not a wrong-shape value keep the parser's wording and
/// gain no shape line: a syntax error, an unknown key, a rejected closed-set
/// value.
#[test]
fn other_errors_get_no_shape_line() -> Result<(), String> {
    for text in [
        "[analysis\nmode = \"draft\"\n",
        "[analysis]\nmod = \"draft\"\n",
        "[analysis]\nmode = \"slow\"\n",
        "[reports]\nmax_related_tests = 0\n",
    ] {
        let message = error(text)?;
        assert!(!message.contains("\nwrite "), "{text}: {message}");
    }
    Ok(())
}
