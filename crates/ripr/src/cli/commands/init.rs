use crate::cli::commands_options::{InitCi, InitOptions};
use crate::cli::help;
use crate::cli::parse::expect_value;
use crate::config::{CONFIG_FILE_NAME, generated_init_config};
use std::path::{Path, PathBuf};

#[path = "init/workflow.rs"]
mod workflow;

#[cfg(test)]
pub(in crate::cli::commands) use workflow::generated_github_actions_workflow;

pub(super) fn init(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        help::print_init_help();
        return Ok(());
    }
    let options = parse_init_options(args)?;
    if options.dry_run {
        print_init_dry_run(&options);
        return Ok(());
    }
    if !options.root.is_dir() {
        return Err(format!(
            "init root {} is not a directory",
            options.root.display()
        ));
    }
    let config_path = options.root.join(CONFIG_FILE_NAME);
    let workflow_path = options
        .ci
        .as_ref()
        .map(|ci| init_ci_workflow_path(&options.root, ci));

    if config_path.exists() && !options.force && options.ci.is_none() {
        return Err(format!(
            "{} already exists; rerun `ripr init --force` to overwrite it",
            config_path.display()
        ));
    }
    if let Some(path) = workflow_path
        .as_ref()
        .filter(|path| path.exists())
        .filter(|_| !options.force)
    {
        return Err(format!(
            "{} already exists; rerun `ripr init --ci github --force` to overwrite it",
            path.display()
        ));
    }

    if config_path.exists() && !options.force {
        println!("Left existing {} unchanged", config_path.display());
    } else {
        std::fs::write(&config_path, generated_init_config())
            .map_err(|err| format!("write {} failed: {err}", config_path.display()))?;
        println!("Wrote {}", config_path.display());
    }

    if let Some(ci) = options.ci.as_ref() {
        write_init_ci_workflow(&options.root, ci)?;
    }
    Ok(())
}

pub(in crate::cli::commands) fn parse_init_options(args: &[String]) -> Result<InitOptions, String> {
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

fn print_init_dry_run(options: &InitOptions) {
    if let Some(ci) = options.ci.as_ref() {
        println!("# {}", CONFIG_FILE_NAME);
        print!("{}", generated_init_config());
        println!();
        println!("# {}", init_ci_workflow_path(&options.root, ci).display());
        print!("{}", workflow::generated_github_actions_workflow());
    } else {
        print!("{}", generated_init_config());
    }
}

fn init_ci_workflow_path(root: &Path, ci: &InitCi) -> PathBuf {
    match ci {
        InitCi::Github => root.join(".github/workflows/ripr.yml"),
    }
}

fn write_init_ci_workflow(root: &Path, ci: &InitCi) -> Result<(), String> {
    match ci {
        InitCi::Github => {
            let path = init_ci_workflow_path(root, ci);
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                std::fs::create_dir_all(parent)
                    .map_err(|err| format!("create {} failed: {err}", parent.display()))?;
            }
            std::fs::write(&path, workflow::generated_github_actions_workflow())
                .map_err(|err| format!("write {} failed: {err}", path.display()))?;
            println!("Wrote {}", path.display());
            Ok(())
        }
    }
}
