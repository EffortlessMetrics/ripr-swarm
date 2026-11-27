//! Operator-only comparison of observed PR heads with one pinned main commit.
pub(super) mod notice;

use super::super::source_promotion::{parse_merge_tree_output, validate_sha};
use crate::run::{ByteCaptureBudget, capture_bytes_in_dir_with_input_env_budget};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

pub(super) const USAGE: &str =
    "cargo xtask merge-queue conflicts <new-output-directory> [--input <saved-queue-pages.json>]";
const REPO: &str = "EffortlessMetrics/ripr-swarm";
const QUERY: &str = r#"query($endCursor:String){repository(owner:"EffortlessMetrics",name:"ripr-swarm"){
  nameWithOwner ref(qualifiedName:"refs/heads/main"){name target{oid}}
  mergeQueue(branch:"main"){resourcePath entries(first:100,after:$endCursor){totalCount pageInfo{hasNextPage endCursor}
  nodes{position pullRequest{number headRefOid}}}}}}"#;
const MAX_FIRST_PARENTS: usize = 64;

#[derive(Serialize)]
struct Row {
    position: usize,
    pr: u64,
    head: String,
    #[serde(flatten)]
    result: Outcome,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Outcome {
    Clean,
    Conflict {
        paths: BTreeMap<String, Option<Boundary>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        unattributed_reason: Option<String>,
    },
    Unexamined {
        reason: String,
    },
}

#[derive(Serialize)]
struct Boundary {
    first_parent: String,
    introduced_at: String,
}

fn parse_pages(bytes: &[u8]) -> Result<(String, Vec<Row>), String> {
    let pages: Vec<Value> = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let mut identity = None;
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    let mut cursors = BTreeSet::new();
    for (index, page) in pages.iter().enumerate() {
        let repo = &page["data"]["repository"];
        let queue = &repo["mergeQueue"]["entries"];
        if page.get("errors").is_some()
            || repo["nameWithOwner"] != REPO
            || repo["ref"]["name"] != "main"
            || repo["mergeQueue"]["resourcePath"] != "/EffortlessMetrics/ripr-swarm/queue/main"
            || queue["pageInfo"]["hasNextPage"].as_bool() != Some(index + 1 < pages.len())
        {
            return Err("incomplete queue pagination or unexpected repository".into());
        }
        if index + 1 < pages.len() {
            let cursor = queue["pageInfo"]["endCursor"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("missing pagination cursor")?;
            if !cursors.insert(cursor) {
                return Err("repeated pagination cursor".into());
            }
        }
        let base = repo["ref"]["target"]["oid"]
            .as_str()
            .ok_or("missing main SHA")?;
        validate_sha("main", "commit", base)?;
        let count = queue["totalCount"].as_u64().ok_or("missing queue count")?;
        if identity.get_or_insert((base.to_owned(), count)) != &(base.to_owned(), count) {
            return Err(
                "main or queue count changed during pagination; retry with a fresh snapshot".into(),
            );
        }
        for entry in queue["nodes"].as_array().ok_or("missing queue entries")? {
            let pr = entry["pullRequest"]["number"]
                .as_u64()
                .ok_or("missing PR number")?;
            let head = entry["pullRequest"]["headRefOid"]
                .as_str()
                .ok_or("missing PR head")?;
            validate_sha("PR head", "commit", head)?;
            let position = rows.len() + 1;
            if pr == 0 || !seen.insert(pr) || entry["position"].as_u64() != Some(position as u64) {
                return Err(
                    "duplicate, missing or reordered queue entry; retry with a fresh snapshot"
                        .into(),
                );
            }
            rows.push(Row {
                position,
                pr,
                head: head.to_owned(),
                result: Outcome::Unexamined {
                    reason: "comparison pending".into(),
                },
            });
        }
    }
    let (base, count) = identity.ok_or("no queue pages")?;
    if rows.len() as u64 != count {
        return Err("incomplete queue census".into());
    }
    Ok((base, rows))
}

#[derive(PartialEq)]
enum Comparison {
    Clean,
    Conflict(BTreeSet<String>),
    UnnamedConflict(&'static str),
}

struct Sweep<'a> {
    objects: &'a Path,
    deadline: Instant,
}

impl Sweep<'_> {
    fn command(
        &self,
        tool: &str,
        args: &[&str],
        input: Option<&[u8]>,
    ) -> Result<(i32, Vec<u8>), String> {
        let left = self.deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err("wall-time budget exhausted".into());
        }
        let args: Vec<_> = args.iter().map(|s| s.to_string()).collect();
        // Isolate Git from caller-supplied merge drivers, hooks and repository overrides.
        let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
        let isolated_git = [
            ("GIT_CONFIG_GLOBAL", null),
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("GIT_CONFIG_COUNT", "1"),
            ("GIT_CONFIG_KEY_0", "core.attributesFile"),
            ("GIT_CONFIG_VALUE_0", null),
            ("GIT_ATTR_NOSYSTEM", "1"),
            ("GIT_NO_REPLACE_OBJECTS", "1"),
            ("GIT_TERMINAL_PROMPT", "0"),
            ("GIT_NO_LAZY_FETCH", "1"),
        ];
        // Fetch can use configured transport or credential helpers. Later object
        // comparisons use only the private bare store and isolated Git configuration.
        let transport = [("GIT_TERMINAL_PROMPT", "0")];
        let is_fetch = tool == "git"
            && matches!(args.as_slice(), [flag, hooks, subcommand, ..] if flag == "-c" && hooks == "core.hooksPath=disabled-hooks/" && subcommand == "fetch");
        let env: &[(&str, &str)] = if is_fetch { &transport } else { &isolated_git };
        let remove = [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_COMMON_DIR",
            "GIT_CONFIG",
            "GIT_CONFIG_PARAMETERS",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_INDEX_FILE",
            "GIT_ATTR_SOURCE",
            "GIT_GRAFT_FILE",
            "GIT_SHALLOW_FILE",
            "GIT_NAMESPACE",
        ];
        let result = capture_bytes_in_dir_with_input_env_budget(
            Path::new(tool),
            &args,
            (self.objects, input),
            (env, &remove),
            ByteCaptureBudget {
                timeout: left,
                stdout_bytes: 8 * 1024 * 1024,
                stderr_bytes: 1024 * 1024,
            },
            "merge-queue conflicts",
        )?;
        if result.timed_out {
            return Err("wall-time budget exhausted".into());
        }
        let code = result
            .status
            .and_then(|s| s.code())
            .ok_or("child did not return an exit code")?;
        Ok((code, result.stdout))
    }

    fn checked(&self, tool: &str, args: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>, String> {
        let (status, stdout) = self.command(tool, args, input)?;
        if status != 0 {
            return Err(format!(
                "{tool} {} exited {status}",
                args.first().unwrap_or(&"")
            ));
        }
        Ok(stdout)
    }

    fn prepare(&self, base: &str, rows: &[Row], remote: &str) -> Result<(), String> {
        self.checked(
            "git",
            &["init", "--bare", "--object-format=sha1", "--template=", "."],
            None,
        )?;
        self.checked("git", &["remote", "add", "origin", remote], None)?;
        let base_ref = format!("+{base}:refs/heads/base\n");
        let head_refs = |rows: &[Row]| {
            rows.iter()
                .map(|r| format!("+{}:refs/heads/pr-{}\n", r.head, r.pr))
                .collect::<String>()
        };
        if self.fetch(&(base_ref.clone() + &head_refs(rows))).is_err() {
            self.fetch(&base_ref)
                .map_err(|reason| format!("fetch pinned main: {reason}"))?;
            // Isolate an unavailable SHA without giving up the rest of the batch.
            let mut batches = vec![rows];
            while let Some(batch) = batches.pop() {
                if Instant::now() >= self.deadline {
                    break;
                }
                if self.fetch(&head_refs(batch)).is_err() && batch.len() > 1 {
                    let (first, second) = batch.split_at(batch.len() / 2);
                    batches.extend([second, first]);
                }
            }
        }
        let (code, _) =
            self.command("git", &[&format!("--attr-source={base}"), "version"], None)?;
        if code != 0 {
            return Err(format!(
                "Git rejected the --attr-source option (exit {code}); use a Git build supporting pinned base attributes"
            ));
        }
        Ok(())
    }

    fn fetch(&self, wants: &str) -> Result<(), String> {
        self.checked(
            "git",
            &[
                "-c",
                "core.hooksPath=disabled-hooks/",
                "fetch",
                "--stdin",
                "--no-tags",
                "--no-recurse-submodules",
                "origin",
            ],
            Some(wants.as_bytes()),
        )
        .map(|_| ())
    }

    fn compare(&self, base: &str, head: &str) -> Result<Comparison, String> {
        let paths = self.merge_result(base, base, head)?;
        // Git labels file/directory relocations with the merge argument. Recompute
        // using stable refs when either argument appears in a path; never rewrite names.
        if matches!(&paths, Comparison::Conflict(paths) if paths.iter().any(|p| p.contains(base) || p.contains(head)))
        {
            self.checked("git", &["update-ref", "--stdin"], Some(format!(
                "update refs/heads/observed-main {base}\nupdate refs/heads/observed-pr {head}\n"
            ).as_bytes()))?;
            return self.merge_result(base, "observed-main", "observed-pr");
        }
        Ok(paths)
    }

    fn merge_result(&self, attr: &str, base: &str, head: &str) -> Result<Comparison, String> {
        let (status, out) = self.command(
            "git",
            &[
                &format!("--attr-source={attr}"),
                "merge-tree",
                "--write-tree",
                "--name-only",
                "-z",
                base,
                head,
            ],
            None,
        )?;
        if status != 0 && status != 1 {
            return Err(format!(
                "git merge-tree --write-tree --name-only -z failed (exit {status}); verify both commit objects and Git support"
            ));
        }
        if status == 1 {
            let mut fields = out.split(|&b| b == 0);
            let tree = fields.next().ok_or("missing merge result")?;
            validate_sha(
                "merged tree",
                "tree",
                std::str::from_utf8(tree).map_err(|e| e.to_string())?,
            )?;
            if fields
                .take_while(|p| !p.is_empty())
                .any(|p| std::str::from_utf8(p).is_err())
            {
                return Ok(Comparison::UnnamedConflict(
                    "Git confirmed a conflict with a non-UTF-8 path; the UTF-8 path inventory is unavailable",
                ));
            }
        }
        let (tree, paths) = parse_merge_tree_output(&out)?;
        validate_sha("merged tree", "tree", &tree)?;
        if status == 0 && !paths.is_empty() {
            return Err("inconsistent merge-tree status and path inventory".into());
        }
        Ok(match (status, paths.is_empty()) {
            (0, _) => Comparison::Clean,
            (_, true) => Comparison::UnnamedConflict(
                "Git confirmed a conflict without individual unmerged paths",
            ),
            _ => Comparison::Conflict(paths.into_iter().collect()),
        })
    }

    fn classify(&self, base: &str, rows: &mut [Row]) {
        for row in rows {
            row.result = match self.compare(base, &row.head) {
                Ok(Comparison::Clean) => Outcome::Clean,
                Ok(Comparison::UnnamedConflict(reason)) => Outcome::Conflict {
                    paths: BTreeMap::new(),
                    unattributed_reason: Some(reason.into()),
                },
                Ok(Comparison::Conflict(paths)) => Outcome::Conflict {
                    paths: paths.into_iter().map(|p| (p, None)).collect(),
                    unattributed_reason: Some("first-parent history not examined".into()),
                },
                Err(reason) => Outcome::Unexamined { reason },
            };
        }
    }

    fn attribute_history(&self, base: &str, rows: &mut [Row]) -> Result<(), String> {
        self.boundaries(base, rows).inspect_err(|reason| {
            for row in rows {
                if let Outcome::Conflict {
                    paths,
                    unattributed_reason,
                } = &mut row.result
                    && paths.values().any(Option::is_none)
                {
                    *unattributed_reason = Some(reason.clone());
                }
            }
        })
    }

    fn boundaries(&self, base: &str, rows: &mut [Row]) -> Result<(), String> {
        let history = self.checked(
            "git",
            &[
                "rev-list",
                "--first-parent",
                &format!("--max-count={}", MAX_FIRST_PARENTS + 1),
                base,
            ],
            None,
        )?;
        let history = std::str::from_utf8(&history).map_err(|e| e.to_string())?;
        for row in rows {
            let Outcome::Conflict {
                paths,
                unattributed_reason,
            } = &mut row.result
            else {
                continue;
            };
            if paths.is_empty() {
                continue;
            }
            *unattributed_reason = Some(format!(
                "no transition in the available {MAX_FIRST_PARENTS} first-parent edges"
            ));
            let mut child = base;
            for parent in history.lines().skip(1) {
                if paths.values().all(Option::is_some) {
                    break;
                }
                let prior = match self.compare(parent, &row.head) {
                    Ok(Comparison::UnnamedConflict(reason)) => {
                        *unattributed_reason = Some(format!("parent: {reason}"));
                        break;
                    }
                    Ok(Comparison::Conflict(prior)) => prior,
                    Ok(Comparison::Clean) => BTreeSet::new(),
                    Err(reason) => {
                        *unattributed_reason = Some(reason);
                        break;
                    }
                };
                if !prior.is_empty()
                    && paths.iter().any(|(path, edge)| {
                        edge.is_none()
                            && !prior.contains(path)
                            && (path.contains("~observed-main") || path.contains("~observed-pr"))
                    })
                {
                    *unattributed_reason = Some("Git relocation name changed while the parent remains conflicted; path identity is ambiguous".into());
                    break;
                }
                for (path, boundary) in paths.iter_mut() {
                    if boundary.is_none() && !prior.contains(path) {
                        *boundary = Some(Boundary {
                            first_parent: parent.to_owned(),
                            introduced_at: child.to_owned(),
                        });
                    }
                }
                child = parent;
            }
            if paths.values().all(Option::is_some) {
                *unattributed_reason = None;
            }
        }
        Ok(())
    }
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    if matches!(args, [flag] if flag == "--help" || flag == "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    let (out, input) = match args {
        [out] if !out.starts_with('-') => (out, None),
        [out, flag, input] if flag == "--input" && !out.starts_with('-') => {
            (out, Some(input.as_str()))
        }
        _ => return Err(USAGE.into()),
    };
    let start = Instant::now();
    let out = Path::new(out);
    with_objects(out, |objects| write_report(out, objects, input, start))
}

fn with_objects(
    out: &Path,
    operation: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let mut directory = fs::DirBuilder::new();
    directory.recursive(false);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut directory, 0o700);
    directory
        .create(out)
        .map_err(|e| format!("{} must be a new output directory: {e}", out.display()))?;
    let objects = out.join("objects.git");
    fs::create_dir(&objects).map_err(|e| e.to_string())?;
    let result = operation(&objects);
    // Both directories were exclusively created by this call. Only the object
    // store is disposable; leave the evidence and any earlier error visible.
    match fs::remove_dir_all(&objects) {
        Ok(()) => result,
        Err(e) => Err(format!(
            "{}could not clean {}: {e}",
            result.err().map(|e| format!("{e}; ")).unwrap_or_default(),
            objects.display()
        )),
    }
}
fn write_report(
    out: &Path,
    objects: &Path,
    input: Option<&str>,
    start: Instant,
) -> Result<(), String> {
    let sweep = Sweep {
        objects,
        deadline: start + Duration::from_mins(8),
    };
    let query = format!("query={QUERY}");
    let snapshot = match input {
        Some(path) => {
            let info = fs::metadata(path).map_err(|e| e.to_string())?;
            if !info.is_file() || info.len() > 8 * 1024 * 1024 {
                return Err("saved queue pages must be a regular file of at most 8 MiB".into());
            }
            fs::read(path).map_err(|e| format!("read saved queue pages: {e}"))?
        }
        None => sweep
            .checked(
                "gh",
                &[
                    "api",
                    "graphql",
                    "--hostname",
                    "github.com",
                    "--paginate",
                    "--slurp",
                    "-f",
                    &query,
                ],
                None,
            )
            .map_err(|e| {
                format!(
                    "{e}; check gh api --help for --slurp support, gh auth status --hostname github.com and private queue read access"
                )
            })?,
    };
    let (base, mut rows) = parse_pages(&snapshot)?;
    fs::write(out.join("queue-pages.json"), snapshot).map_err(|e| e.to_string())?;
    let mut notes = Vec::new();
    if !rows.is_empty() {
        match sweep.prepare(
            &base,
            &rows,
            "git@github.com:EffortlessMetrics/ripr-swarm.git",
        ) {
            Ok(()) => {
                sweep.classify(&base, &mut rows);
                if let Err(reason) = sweep.attribute_history(&base, &mut rows) {
                    notes.push(reason);
                }
            }
            Err(reason) => {
                for row in &mut rows {
                    row.result = Outcome::Unexamined {
                        reason: reason.clone(),
                    };
                }
            }
        }
    }
    let incomplete = rows
        .iter()
        .any(|r| matches!(r.result, Outcome::Unexamined { .. }));
    let source = if input.is_some() {
        "operator_supplied"
    } else {
        "github_api"
    };
    let report = json!({
        "schema_version": 1,
        "repository": REPO,
        "base": base,
        "comparison": "PR head against pinned main",
        "snapshot_source": source,
        "history_first_parent_limit": MAX_FIRST_PARENTS,
        "wall_time_budget_seconds": 480,
        "elapsed_ms": start.elapsed().as_millis(),
        "notes": notes,
        "entries": rows
    });
    let path = out.join("queue-conflicts.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!("Wrote {}", path.display());
    if incomplete {
        return Err("some queue heads were unexamined; inspect the saved report".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
