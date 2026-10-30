#[derive(Debug)]
struct ExactSelection {
    package: String,
    test: Option<String>,
}
enum SelectionRequest {
    OutsideScope,
    Exact(ExactSelection),
    Unresolved(&'static str),
}

fn selection_request(argv: &[String]) -> SelectionRequest {
    if argv.first().map(String::as_str) != Some("cargo")
        || argv.get(1).map(String::as_str) != Some("test")
    {
        return SelectionRequest::OutsideScope;
    }
    let (mut packages, mut tests) = (Vec::new(), Vec::new());
    let (mut locked, mut context_sensitive, mut mixed) = (false, false, false);
    let mut unknown_option = false;
    let mut index = 2;
    while let Some(arg) = argv.get(index) {
        if arg == "--" {
            break;
        }
        match arg.as_str() {
            "--locked" | "--frozen" => locked = true,
            "-p" | "--package" | "--test" => {
                let Some(value) = argv.get(index + 1) else {
                    return SelectionRequest::Unresolved("selector value is missing");
                };
                if arg == "--test" {
                    tests.push(value.clone());
                } else {
                    packages.push(value.clone());
                }
                index += 1;
            }
            "--features" | "--target" => {
                context_sensitive = true;
                index += 1;
            }
            "--all-features" | "--no-default-features" => context_sensitive = true,
            "--workspace" => context_sensitive = true,
            "--all-targets" | "--tests" | "--lib" | "--bins" | "--examples" | "--doc" => {
                mixed = true
            }
            "--bin" | "--example" => {
                mixed = true;
                index += 1;
            }
            _ if arg.starts_with("--package=") => {
                packages.push(arg["--package=".len()..].to_owned())
            }
            _ if arg.starts_with("--test=") => tests.push(arg["--test=".len()..].to_owned()),
            _ if arg.starts_with("--features=") || arg.starts_with("--target=") => {
                context_sensitive = true
            }
            _ if arg.starts_with("--bin=") || arg.starts_with("--example=") => mixed = true,
            "--offline" => {}
            _ if arg.starts_with('-') => unknown_option = true,
            _ => {}
        }
        index += 1;
    }
    if tests.is_empty() && packages.is_empty() {
        return SelectionRequest::OutsideScope;
    }
    if !locked
        || unknown_option
        || (!tests.is_empty() && mixed)
        || packages.len() != 1
        || tests.len() > 1
    {
        return SelectionRequest::Unresolved(
            "requires one locked package and integration-test selector without mixed selection",
        );
    }
    let literal = |value: &str| {
        !value.is_empty()
            && value
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    };
    if !literal(&packages[0]) || tests.first().is_some_and(|test| !literal(test)) {
        return SelectionRequest::Unresolved(
            "glob, version, or nonliteral selector is outside this exact-selection adapter",
        );
    }
    if context_sensitive {
        return SelectionRequest::Unresolved(
            "feature/platform-sensitive selection is preserved but not resolved by this default-context adapter",
        );
    }
    SelectionRequest::Exact(ExactSelection {
        package: packages.remove(0),
        test: tests.pop(),
    })
}

#[cfg(test)]
mod tests;
