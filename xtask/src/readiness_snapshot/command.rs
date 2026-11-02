use std::env;
use std::path::PathBuf;

use super::build::{BuildInput, build_snapshot};
use super::render::{render_json, render_markdown};

const JSON_NAME: &str = "readiness-snapshot.json";
const MD_NAME: &str = "readiness-snapshot.md";

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let options = parse_args(args)?;
    let root = env::current_dir().map_err(|err| format!("failed to read cwd: {err}"))?;
    let snapshot = build_snapshot(&BuildInput {
        root: &root,
        sha: &options.sha,
        rehearsal_receipt: options.rehearsal_receipt.as_deref(),
    })?;
    let json = render_json(&snapshot)?;
    let markdown = render_markdown(&snapshot);
    if !options.json_only {
        repo_policy::write_report(MD_NAME, &markdown)?;
    }
    if options.json || options.json_only {
        repo_policy::write_report(JSON_NAME, &json)?;
    }
    if !options.json && !options.json_only {
        repo_policy::write_report(JSON_NAME, &json)?;
    }
    Ok(())
}

struct Options {
    sha: String,
    rehearsal_receipt: Option<PathBuf>,
    json: bool,
    json_only: bool,
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut sha = "HEAD".to_string();
    let mut rehearsal_receipt = None;
    let mut json = false;
    let mut json_only = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--sha" => {
                sha = require_value(args, index, "--sha")?;
                index += 2;
            }
            "--rehearsal-receipt" => {
                rehearsal_receipt = Some(PathBuf::from(require_value(
                    args,
                    index,
                    "--rehearsal-receipt",
                )?));
                index += 2;
            }
            "--json" => {
                json = true;
                index += 1;
            }
            "--json-only" => {
                json_only = true;
                json = true;
                index += 1;
            }
            "--help" | "-h" => return Err(usage()),
            other => {
                return Err(format!(
                    "unknown readiness-snapshot argument `{other}`; {}",
                    usage()
                ));
            }
        }
    }
    Ok(Options {
        sha,
        rehearsal_receipt,
        json,
        json_only,
    })
}

fn require_value(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index + 1)
        .cloned()
        .ok_or_else(|| format!("missing value for {flag}; {}", usage()))
}

fn usage() -> String {
    "usage: cargo xtask readiness-snapshot --sha <rev> [--rehearsal-receipt <path>] [--json] [--json-only]".to_string()
}
