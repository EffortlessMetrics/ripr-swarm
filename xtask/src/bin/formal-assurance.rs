//! Passive registry validation, currentness join and scoreboard front door.
use std::path::Path;
use xtask::formal_evidence::{self, AuditResult, Run, require};
use xtask::formal_registry::{self as registry, REGISTRY_PATH, USAGE};

fn execute(args: &[String]) -> AuditResult<String> {
    let mut options = std::collections::BTreeMap::new();
    let mut changed = Vec::new();
    let mut pairs = Vec::new();
    let mut args = args.iter();
    while let Some(key) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("{key} requires a value; {USAGE}"))?;
        if key == "--changed-owner" {
            changed.push(value.clone());
            continue;
        }
        if key == "--control-pair" {
            pairs.push(std::path::PathBuf::from(value));
            continue;
        }
        require(
            matches!(
                key.as_str(),
                "--root"
                    | "--registry"
                    | "--run"
                    | "--artifacts"
                    | "--previous-root"
                    | "--previous-registry"
                    | "--format"
                    | "--check-output"
            ),
            "unknown formal-assurance option",
        )?;
        require(
            options.insert(key.as_str(), value.as_str()).is_none(),
            "duplicate formal-assurance option",
        )?;
    }
    let root = Path::new(options.get("--root").copied().ok_or("--root is required")?);
    let prepared = registry::prepare(
        root,
        Path::new(options.get("--registry").copied().unwrap_or(REGISTRY_PATH)),
    )?;
    require(
        options.contains_key("--previous-root") == options.contains_key("--previous-registry"),
        "predecessor needs both registry and source root",
    )?;
    let previous = options
        .get("--previous-root")
        .zip(options.get("--previous-registry"))
        .map(|(root, path)| registry::prepare(Path::new(root), Path::new(path)))
        .transpose()?;
    require(
        !options.contains_key("--artifacts") || options.contains_key("--run"),
        "artifacts require a run",
    )?;
    let run: Option<Run> = options
        .get("--run")
        .map(|p| {
            registry::read_input(Path::new(p)).and_then(|bytes| formal_evidence::parse_json(&bytes))
        })
        .transpose()?;
    let board = registry::scoreboard_with_control_pairs(
        &prepared,
        previous.as_ref(),
        run.as_ref()
            .map(|r| (r, options.get("--artifacts").map(Path::new))),
        &changed,
        &pairs,
    )?;
    let output = match options.get("--format").copied().unwrap_or("json") {
        "json" => serde_json::to_string_pretty(&board).map_err(|e| e.to_string()),
        "markdown" => Ok(registry::render_markdown(&board)),
        "claim-index" => serde_json::to_string_pretty(&registry::claim_index(&prepared)?)
            .map_err(|e| e.to_string()),
        _ => Err("format must be json, markdown, or claim-index".to_string()),
    }?;
    if let Some(path) = options.get("--check-output") {
        let prior =
            String::from_utf8(registry::read_input(Path::new(path))?).map_err(|e| e.to_string())?;
        require(
            prior.trim_end() == output.trim_end(),
            "generated formal-assurance output is stale",
        )?;
    }
    Ok(output)
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--help"] || args == ["-h"] {
        println!("{USAGE}");
        return std::process::ExitCode::SUCCESS;
    }
    match execute(&args) {
        Ok(output) => {
            println!("{output}");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("formal-assurance: {error}");
            std::process::ExitCode::from(2)
        }
    }
}
