//! Dispatch for `ripr capabilities`.
//!
//! CLI adapter only: the table lives in `crate::domain::language_capability`
//! and the rendering in `crate::output::language_capabilities`.

use crate::cli::help;
use crate::cli::suggest::{unknown_argument, unknown_value};
use crate::domain::LanguageId;
use crate::output;

pub(in crate::cli) fn capabilities(args: &[String]) -> Result<(), String> {
    let mut json_output = false;
    let mut language: Option<LanguageId> = None;
    let mut arguments = args.iter();
    while let Some(arg) = arguments.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                help::print_capabilities_help();
                return Ok(());
            }
            "--json" => json_output = true,
            "--language" => {
                let value = match arguments.next().map(String::as_str) {
                    Some("--help" | "-h" | "--json" | "--language") | None => {
                        return Err("missing value for --language".to_string());
                    }
                    Some(value) => value,
                };
                if language.is_some() {
                    return Err(
                        "capabilities accepts at most one --language. Run `ripr capabilities --help`."
                            .to_string(),
                    );
                }
                language = Some(LanguageId::from_wire(value).ok_or_else(|| {
                    let accepted: Vec<&str> =
                        LanguageId::ALL.iter().map(LanguageId::as_str).collect();
                    unknown_value("language", value, &accepted)
                })?);
            }
            other if other.starts_with('-') => return Err(unknown_argument("capabilities", other)),
            other => {
                return Err(format!(
                    "capabilities does not accept positional arguments; got {other:?}. Run `ripr capabilities --help`."
                ));
            }
        }
    }
    if json_output {
        print!("{}", output::language_capabilities::render_json(language));
    } else {
        print!("{}", output::language_capabilities::render_human(language));
    }
    Ok(())
}
