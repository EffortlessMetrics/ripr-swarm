//! `ripr pr-comments existing` and `ripr pr-comments requests` (#5409): the
//! JSON work on either side of the generated workflow's `gh api` calls.
//! Neither command reads a token or the network; the workflow step that
//! holds `GH_TOKEN` makes every call.

use crate::cli::parse::expect_value;
use crate::cli::suggest::unknown_argument;
use crate::output::pr_inline_comment_github::{existing_comments, publish_requests};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

use super::non_empty_string_arg;

const EXISTING: &str = "pr-comments existing";
const REQUESTS: &str = "pr-comments requests";
const DEFAULT_RAW: &str = "target/ripr/review/existing-comments.raw.json";
const DEFAULT_EXISTING_OUT: &str = "target/ripr/review/existing-comments.json";
const DEFAULT_PLAN: &str = "target/ripr/review/comment-publish-plan.json";
const DEFAULT_REQUESTS_DIR: &str = "target/ripr/review/publish";
/// One line per call, in order: method, endpoint after `repos/OWNER/REPO/`,
/// request file, and the message to print once the call succeeds,
/// separated by tabs.
pub(crate) const REQUESTS_MANIFEST: &str = "requests.tsv";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ExistingOptions {
    pub(super) root: PathBuf,
    pub(super) raw: PathBuf,
    pub(super) out: PathBuf,
}

pub(super) fn parse_pr_comments_existing_options(
    args: &[String],
) -> Result<ExistingOptions, String> {
    let mut options = ExistingOptions {
        root: PathBuf::from("."),
        raw: PathBuf::from(DEFAULT_RAW),
        out: PathBuf::from(DEFAULT_EXISTING_OUT),
    };
    let mut i = 0usize;
    while i < args.len() {
        let flag = args[i].as_str();
        i += 1;
        match flag {
            "--root" => {
                options.root = PathBuf::from(non_empty_string_arg(args, i, flag, EXISTING)?)
            }
            "--raw" => options.raw = PathBuf::from(non_empty_string_arg(args, i, flag, EXISTING)?),
            "--out" => options.out = PathBuf::from(non_empty_string_arg(args, i, flag, EXISTING)?),
            other => return Err(unknown_argument(EXISTING, other)),
        }
        i += 1;
    }
    Ok(options)
}

pub(super) fn pr_comments_existing(args: &[String]) -> Result<(), String> {
    let options = parse_pr_comments_existing_options(args)?;
    let pages = if options.raw.as_os_str() == "-" {
        // The workflow pipes `gh api` straight in; keep the raw pages beside
        // the normalized file so the uploaded artifact still shows them.
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut std::io::stdin(), &mut bytes)
            .map_err(|err| format!("{EXISTING} could not read standard input: {err}"))?;
        let raw_copy = options.root.join(DEFAULT_RAW);
        if let Some(parent) = raw_copy.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                format!("{EXISTING} could not create {}: {err}", parent.display())
            })?;
        }
        fs::write(&raw_copy, &bytes)
            .map_err(|err| format!("{EXISTING} could not write {}: {err}", raw_copy.display()))?;
        serde_json::from_slice(&bytes)
            .map_err(|err| format!("{EXISTING}: standard input is not JSON: {err}"))?
    } else {
        read_json(&options.root.join(&options.raw), EXISTING)?
    };
    let existing = existing_comments(&pages);
    let out = options.root.join(&options.out);
    write_json(&out, &existing, EXISTING)?;
    let count = existing["comments"].as_array().map_or(0, Vec::len);
    println!(
        "Captured {count} existing RIPR inline comment(s) in {}.",
        options.out.display()
    );
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RequestsOptions {
    pub(super) root: PathBuf,
    pub(super) plan: PathBuf,
    pub(super) out_dir: PathBuf,
    pub(super) pull_request: String,
    pub(super) head_sha: String,
}

pub(super) fn parse_pr_comments_requests_options(
    args: &[String],
) -> Result<RequestsOptions, String> {
    let mut options = RequestsOptions {
        root: PathBuf::from("."),
        plan: PathBuf::from(DEFAULT_PLAN),
        out_dir: PathBuf::from(DEFAULT_REQUESTS_DIR),
        pull_request: String::new(),
        head_sha: String::new(),
    };
    let mut i = 0usize;
    while i < args.len() {
        let flag = args[i].as_str();
        i += 1;
        match flag {
            "--root" => {
                options.root = PathBuf::from(non_empty_string_arg(args, i, flag, REQUESTS)?)
            }
            "--plan" => {
                options.plan = PathBuf::from(non_empty_string_arg(args, i, flag, REQUESTS)?)
            }
            "--out-dir" => {
                options.out_dir = PathBuf::from(non_empty_string_arg(args, i, flag, REQUESTS)?);
            }
            "--pull-request" => options.pull_request = expect_value(args, i, flag)?.to_string(),
            "--head-sha" => options.head_sha = expect_value(args, i, flag)?.to_string(),
            other => return Err(unknown_argument(REQUESTS, other)),
        }
        i += 1;
    }
    // Both land in an API path or payload; refuse anything but the shapes
    // GitHub uses so a request can never address another endpoint.
    if options.pull_request.is_empty() || !options.pull_request.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(format!(
            "{REQUESTS} needs --pull-request N with the pull request number (got {:?}); in the generated workflow that is ${{{{ github.event.pull_request.number }}}}",
            options.pull_request
        ));
    }
    if options.head_sha.is_empty() || !options.head_sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!(
            "{REQUESTS} needs --head-sha SHA with the pull request head commit (got {:?}); in the generated workflow that is ${{{{ github.event.pull_request.head.sha }}}}",
            options.head_sha
        ));
    }
    Ok(options)
}

pub(super) fn pr_comments_requests(args: &[String]) -> Result<(), String> {
    let options = parse_pr_comments_requests_options(args)?;
    let plan = read_json(&options.root.join(&options.plan), REQUESTS)?;
    let planned = publish_requests(&plan, &options.pull_request, &options.head_sha);

    let out_dir = options.root.join(&options.out_dir);
    // Stale request files from an earlier run must never be replayed.
    if out_dir.exists() {
        fs::remove_dir_all(&out_dir)
            .map_err(|err| format!("{REQUESTS} could not clear {}: {err}", out_dir.display()))?;
    }
    fs::create_dir_all(&out_dir)
        .map_err(|err| format!("{REQUESTS} could not create {}: {err}", out_dir.display()))?;
    let mut manifest = String::new();
    for (index, request) in planned.requests.iter().enumerate() {
        let file = options.out_dir.join(format!(
            "{:02}-{}.json",
            index + 1,
            request.method.to_ascii_lowercase()
        ));
        write_json(&options.root.join(&file), &request.payload, REQUESTS)?;
        manifest.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            request.method,
            request.endpoint,
            file.display(),
            request.message
        ));
    }
    let manifest_path = out_dir.join(REQUESTS_MANIFEST);
    fs::write(&manifest_path, manifest).map_err(|err| {
        format!(
            "{REQUESTS} could not write {}: {err}",
            manifest_path.display()
        )
    })?;
    for note in &planned.notes {
        println!("{note}");
    }
    Ok(())
}

fn read_json(path: &Path, command: &str) -> Result<Value, String> {
    let bytes = fs::read(path)
        .map_err(|err| format!("{command} could not read {}: {err}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|err| format!("{command}: {} is not JSON: {err}", path.display()))
}

fn write_json(path: &Path, value: &Value, command: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("{command} could not create {}: {err}", parent.display()))?;
    }
    let mut text = serde_json::to_string_pretty(value)
        .map_err(|err| format!("{command} could not render {}: {err}", path.display()))?;
    text.push('\n');
    fs::write(path, text)
        .map_err(|err| format!("{command} could not write {}: {err}", path.display()))
}
