//! Source-shape oracle for the precommit command catalogue.

pub(crate) fn require_catalogue(source: &str, expected: &[String]) -> Result<(), String> {
    let start = source
        .find("\nfn precommit() -> Result<(), String> {")
        .ok_or("xtask/src/main.rs must define `fn precommit()`")?;
    let body = &source[start..];
    let end = body
        .find("precommit_report_body")
        .ok_or("precommit body extraction must stop before `precommit_report_body`")?;
    let executed = body[..end]
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with("//") {
                return None;
            }
            let call = line
                .strip_suffix("()?;")
                .or_else(|| line.strip_suffix("()?,"))?;
            let call = call
                .rsplit_once("=>")
                .map_or(call, |(_, expression)| expression)
                .trim();
            if call == "markdown_links" || call.starts_with("check_") {
                Some(call.replace('_', "-"))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if executed != expected {
        return Err(format!(
            "gates executed by `precommit()` {executed:?} must match PRECOMMIT_GATE_COMMANDS {expected:?}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::require_catalogue;

    fn source(workflow: &str) -> String {
        format!(
            "\nfn precommit() -> Result<(), String> {{\n    check_static_language()?;\n{workflow}\n    markdown_links()?;\n    let body = precommit_report_body();\n}}\n"
        )
    }

    fn expected() -> Vec<String> {
        ["check-static-language", "check-workflows", "markdown-links"]
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn standalone_and_inline_match_arm_preserve_the_same_gate() -> Result<(), String> {
        for workflow in [
            "    check_workflows()?;",
            "    match receipt {\n        Some(path) => verify(path)?,\n        None => check_workflows()?,\n    }",
            "    match receipt {\n        None => {\n            check_workflows()?;\n        }\n    }",
        ] {
            require_catalogue(&source(workflow), &expected())?;
        }
        Ok(())
    }

    #[test]
    fn genuinely_omitted_gates_fail_for_both_supported_shapes() -> Result<(), String> {
        for workflow in [
            "    check_workflows()?;",
            "    match receipt {\n        None => check_workflows()?,\n    }",
        ] {
            let complete = source(workflow);
            for omitted in ["check_static_language", "check_workflows", "markdown_links"] {
                let wrong = complete.replace(&format!("{omitted}()"), "unrelated()");
                if require_catalogue(&wrong, &expected()).is_ok() {
                    return Err(format!("oracle accepted omitted {omitted}: {wrong}"));
                }
            }
        }
        Ok(())
    }

    #[test]
    fn comments_extra_gates_and_reordered_gates_cannot_satisfy_catalogue() -> Result<(), String> {
        let comment = source("    // check_workflows()?;");
        let extra = source("    check_workflows()?;\n    check_unexpected()?;");
        let arm_comment = source("    // None => check_workflows()?,");
        let reordered = source("    check_workflows()?;").replace(
            "    check_static_language()?;\n    check_workflows()?;",
            "    check_workflows()?;\n    check_static_language()?;",
        );
        for wrong in [comment, arm_comment, extra, reordered] {
            if require_catalogue(&wrong, &expected()).is_ok() {
                return Err(format!("oracle accepted catalogue drift: {wrong}"));
            }
        }
        Ok(())
    }
}
