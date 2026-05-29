use crate::cli::commands_options::{InitCi, InitOptions};
use crate::cli::parse::expect_value;
use std::path::PathBuf;

pub(super) fn parse_init_options(args: &[String]) -> Result<InitOptions, String> {
    let mut options = InitOptions {
        root: PathBuf::from("."),
        dry_run: false,
        force: false,
        ci: None,
    };
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                i += 1;
                options.root = PathBuf::from(expect_value(args, i, "--root")?);
            }
            "--ci" => {
                i += 1;
                options.ci = Some(parse_init_ci(expect_value(args, i, "--ci")?)?);
            }
            "--dry-run" => options.dry_run = true,
            "--force" => options.force = true,
            other => return Err(format!("unknown init argument {other:?}")),
        }
        i += 1;
    }
    Ok(options)
}

fn parse_init_ci(value: &str) -> Result<InitCi, String> {
    match value {
        "github" => Ok(InitCi::Github),
        _ => Err(format!("unknown init --ci provider {value:?}")),
    }
}
