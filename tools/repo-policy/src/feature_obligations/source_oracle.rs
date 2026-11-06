//! Test-only, strict checked-in-source comparison. Not a YAML interpreter.
use super::super::*;
use std::collections::BTreeSet;

#[path = "source_controls.rs"]
mod source_controls;

/// Accept only the plain-key/block layout this oracle actually understands.
/// Quoted/escaped/explicit keys, aliases, flow maps and duplicate keys must not
/// become invisible fields. Block-scalar contents are data, not YAML keys.
fn plain_mapping_shape(source: &str) -> Result<(), String> {
    let mut scalar_indent = None;
    let mut scopes: Vec<(usize, BTreeSet<&str>)> = Vec::new();
    for line in source.lines() {
        let indent = line.len() - line.trim_start_matches(' ').len();
        let text = &line[indent..];
        if text.is_empty() || text.starts_with('#') {
            continue;
        }
        if scalar_indent.is_some_and(|parent| indent > parent) {
            continue;
        }
        scalar_indent = None;
        let (key_indent, mapping, sequence) = match text.strip_prefix("- ") {
            Some(mapping) => (indent + 2, mapping, true),
            None => (indent, text, false),
        };
        let Some((key, value)) = mapping.split_once(':') else {
            return Err("unsupported nonplain YAML key or mapping".to_owned());
        };
        if key.is_empty()
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err("unsupported nonplain YAML key or mapping".to_owned());
        }
        if !value.is_empty() && !value.starts_with(' ') {
            return Err("unsupported YAML mapping delimiter".to_owned());
        }
        scopes.retain(|(depth, _)| *depth < key_indent || (!sequence && *depth == key_indent));
        if scopes.last().is_none_or(|(depth, _)| *depth != key_indent) {
            scopes.push((key_indent, BTreeSet::new()));
        }
        let (_, keys) = scopes.last_mut().ok_or("missing mapping scope")?;
        if !keys.insert(key) {
            return Err(format!("duplicate YAML key: {key}"));
        }
        match value.trim() {
            "|" | ">-" => scalar_indent = Some(key_indent),
            value if value.starts_with(['|', '>', '&', '*', '!', '{']) => {
                return Err("unsupported YAML value shape".to_owned());
            }
            _ => {}
        }
    }
    Ok(())
}

fn mapping_keys(source: &str, indent: usize) -> BTreeSet<&str> {
    let prefix = " ".repeat(indent);
    source
        .lines()
        .filter_map(|line| {
            let line = line.strip_prefix(&prefix)?;
            if line.starts_with([' ', '#']) {
                return None;
            }
            line.split_once(':').map(|(key, _)| key)
        })
        .collect()
}

fn allowed_keys(
    source: &str,
    indent: usize,
    permitted: &[&str],
    owner: &str,
) -> Result<(), String> {
    for key in mapping_keys(source, indent) {
        if !permitted.contains(&key) {
            return Err(format!("unsupported {owner} key: {key}"));
        }
    }
    Ok(())
}

fn job_execution_context(source: &str, child: &Child) -> Result<(), String> {
    let header = source.split("    steps:\n").next().unwrap_or(source);
    allowed_keys(
        header,
        4,
        &[
            "name",
            "needs",
            "if",
            "runs-on",
            "timeout-minutes",
            "defaults",
        ],
        "producer job",
    )?;
    let has_defaults = mapping_keys(header, 4).contains("defaults");
    if child.working_directory == "." {
        if has_defaults {
            return Err("unsupported job defaults".to_owned());
        }
    } else {
        let (_, defaults) = header
            .split_once("    defaults:\n")
            .ok_or("missing nested defaults")?;
        same(
            defaults.trim_end(),
            "      run:\n        working-directory: editors/vscode",
            "nested defaults",
        )?;
    }
    Ok(())
}

// A narrow test oracle for the checked-in YAML spelling, not a YAML parser or
// production policy/admission API. Ambiguous owners refuse instead of choosing
// whichever occurrence makes a mutation appear correct.
pub(super) fn job<'a>(source: &'a str, name: &str) -> Result<&'a str, String> {
    let (_, jobs) = source.split_once("\njobs:\n").ok_or("missing jobs")?;
    let marker = format!("  {name}:\n");
    let mut matches = jobs.match_indices(&marker);
    let (offset, _) = matches
        .next()
        .ok_or_else(|| format!("missing job {name}"))?;
    if matches.next().is_some() {
        return Err(format!("duplicate job {name}"));
    }
    let tail = &jobs[offset + marker.len()..];
    let end = tail
        .match_indices('\n')
        .find_map(|(offset, _)| {
            let next = &tail[offset + 1..];
            (next.starts_with("  ") && !next.starts_with("   ") && !next.starts_with("  #"))
                .then_some(offset)
        })
        .unwrap_or(tail.len());
    Ok(&tail[..end])
}

fn steps(source: &str) -> Vec<String> {
    source
        .split("      - ")
        .skip(1)
        .map(|step| {
            // Normalize only the first line's sequence marker away.
            format!("        {step}")
        })
        .collect()
}

pub(super) fn field(source: &str, indent: usize, key: &str) -> Option<String> {
    let prefix = format!("{}{key}: ", " ".repeat(indent));
    let lines = source.lines().collect::<Vec<_>>();
    let index = lines.iter().position(|line| line.starts_with(&prefix))?;
    let value = lines[index].strip_prefix(&prefix)?;
    if value != "|" && value != ">-" {
        return Some(value.to_owned());
    }
    let continuation = " ".repeat(indent + 2);
    let body = lines[index + 1..]
        .iter()
        .take_while(|line| line.starts_with(&continuation) || line.is_empty())
        .map(|line| line.strip_prefix(&continuation).unwrap_or(""))
        .collect::<Vec<_>>();
    Some(
        body.join(if value == ">-" { " " } else { "\n" })
            .trim_end()
            .to_owned(),
    )
}

fn named_step(source: &str, name: &str) -> Result<String, String> {
    let matching = steps(source)
        .into_iter()
        .filter(|step| field(step, 8, "name").as_deref() == Some(name))
        .collect::<Vec<_>>();
    match matching.as_slice() {
        [step] => Ok(step.clone()),
        _ => Err(format!("missing or ambiguous step {name}")),
    }
}

pub(super) fn same<T: std::fmt::Debug + PartialEq>(
    actual: T,
    expected: T,
    owner: &str,
) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!("{owner}: actual {actual:?}, expected {expected:?}"))
    }
}

pub(super) fn source_agrees(ci: &str, vscode: &str) -> Result<(), String> {
    for source in [ci, vscode] {
        plain_mapping_shape(source)?;
        allowed_keys(
            source.split("\njobs:\n").next().unwrap_or(source),
            0,
            &["name", "on", "permissions", "concurrency"],
            "workflow",
        )?;
    }
    let inventory = source_inventory();
    for child in &inventory.children {
        let caller = job(ci, child.caller_job)?;
        same(
            field(caller, 4, "if"),
            Some(child.condition.clone()),
            child.caller_job,
        )?;
        same(
            field(caller, 4, "needs"),
            Some(
                if child.caller_job == "vscode" {
                    "[source-identity]"
                } else {
                    "source-identity"
                }
                .to_owned(),
            ),
            "source-identity dependency",
        )?;
        let source = if child.producer.workflow == CI_WORKFLOW {
            ci
        } else {
            vscode
        };
        let producer = job(source, child.producer.job)?;
        job_execution_context(producer, child)?;
        same(
            field(producer, 4, "runs-on"),
            Some(child.runner.to_owned()),
            child.caller_job,
        )?;
        if child.working_directory != "." {
            same(
                field(producer, 8, "working-directory"),
                Some(child.working_directory.to_owned()),
                "nested cwd",
            )?;
        }
        let mut observed = Vec::new();
        for step in steps(producer) {
            if let Some(run) = field(&step, 8, "run") {
                let name = field(&step, 8, "name");
                if child.caller_job == "rust-only-features"
                    && name.as_deref() == Some("Detect Rust surface change")
                {
                    continue;
                }
                allowed_keys(&step, 8, &["name", "run", "if", "shell"], "command step")?;
                let expected_shell = child
                    .commands
                    .iter()
                    .find(|command| command.name.map(str::to_owned) == name && command.run == run)
                    .and_then(|command| command.shell)
                    .map(str::to_owned);
                same(field(&step, 8, "shell"), expected_shell, "command shell")?;
                observed.push((name, run, field(&step, 8, "if")));
            }
        }
        let expected = child
            .commands
            .iter()
            .map(|command| {
                (
                    command.name.map(str::to_owned),
                    command.run.to_owned(),
                    command.condition.map(str::to_owned),
                )
            })
            .collect::<Vec<_>>();
        same(observed, expected, child.caller_job)?;
        if producer.contains("continue-on-error:")
            || producer.contains("strategy:")
            || producer.contains("outputs:")
        {
            return Err(format!(
                "unmodeled execution/result structure in {}",
                child.caller_job
            ));
        }
    }
    let surface = named_step(job(ci, "rust-only-features")?, "Detect Rust surface change")?;
    same(
        field(&surface, 8, "id"),
        Some("surface".to_owned()),
        "surface owner",
    )?;
    same(
        surface.trim_end(),
        source_controls::SURFACE_STEP,
        "complete surface step",
    )?;
    let call = &inventory.nested_call;
    let caller = job(ci, call.caller.job)?;
    same(
        field(caller, 4, "uses"),
        Some(format!("./{}", call.callee.workflow)),
        "nested workflow",
    )?;
    same(
        field(caller, 6, "ref"),
        Some(call.ref_expression.to_owned()),
        "nested ref",
    )?;
    for input in call.budget_inputs {
        same(
            field(caller, 6, input),
            Some(format!("${{{{ inputs.{input} }}}}")),
            input,
        )?;
    }
    same(
        field(&steps(job(vscode, "vscode")?)[0], 10, "ref"),
        Some("${{ inputs.ref || github.sha }}".to_owned()),
        "nested checkout",
    )?;
    for upload in &inventory.uploads {
        let source = if upload.owner.workflow == CI_WORKFLOW {
            ci
        } else {
            vscode
        };
        let step = named_step(job(source, upload.owner.job)?, upload.step)?;
        same(
            field(&step, 8, "if"),
            Some(upload.condition.to_owned()),
            upload.step,
        )?;
        same(
            field(&step, 10, "name"),
            Some(upload.name.to_owned()),
            upload.step,
        )?;
        same(
            field(&step, 10, "path"),
            Some(upload.paths.join("\n")),
            upload.step,
        )?;
        same(
            field(&step, 10, "if-no-files-found"),
            Some(upload.missing_files.to_owned()),
            upload.step,
        )?;
        same(
            field(&step, 10, "retention-days"),
            Some(upload.retention_days.to_string()),
            upload.step,
        )?;
        same(
            field(&step, 8, "id").is_some(),
            upload.exports_artifact_identity,
            upload.step,
        )?;
    }
    let aggregate = &inventory.aggregate;
    let result = job(ci, aggregate.owner.job)?;
    allowed_keys(
        result.split("    steps:\n").next().unwrap_or(result),
        4,
        &["if", "needs", "runs-on", "timeout-minutes"],
        "aggregate job",
    )?;
    same(
        field(result, 4, "if"),
        Some(aggregate.condition.to_owned()),
        "result condition",
    )?;
    same(
        field(result, 4, "needs"),
        Some(aggregate.needs.to_owned()),
        "result needs",
    )?;
    let step = named_step(result, aggregate.step)?;
    same(
        step.trim_end(),
        source_controls::AGGREGATE_STEP,
        "complete aggregate step",
    )?;
    let cancellation = named_step(result, "Reject workflow cancellation")?;
    same(
        cancellation.trim_end(),
        "        name: Reject workflow cancellation\n        if: always() && cancelled()\n        run: exit 1",
        "complete cancellation step",
    )?;
    same(
        aggregate.accepted_pairs,
        &["true:success", "false:skipped"][..],
        "aggregate accepted source pairs",
    )?;
    for edge in aggregate.edges {
        same(
            field(&step, 10, edge.env),
            Some(format!("${{{{ needs.{}.result }}}}", edge.child_job)),
            "result owner",
        )?;
        let child = inventory
            .children
            .iter()
            .find(|child| child.caller_job == edge.child_job)
            .ok_or("unknown result child")?;
        same(
            field(&step, 10, edge.selection_env),
            Some(format!("${{{{ inputs.{} }}}}", child.selection_input)),
            "selection owner",
        )?;
    }
    Ok(())
}
