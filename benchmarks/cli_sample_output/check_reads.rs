//! Executable Linux strace witness for #7825 (compile with rustc --edition 2024).
//! No analyzer output is synthesized. Use one --exact smoke case per trace.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn events(trace: &str) -> Result<Vec<String>, String> {
    let mut pending = BTreeMap::new();
    let mut events = Vec::new();
    for line in trace.lines() {
        let Some((pid, rest)) = line.split_once(' ') else {
            continue;
        };
        if let Some(prefix) = line.strip_suffix("<unfinished ...>") {
            if pending
                .insert(pid.to_string(), prefix.to_string())
                .is_some()
            {
                return Err(format!("overlapping unfinished call for {pid}"));
            }
        } else if rest.trim_start().starts_with("<... ") && rest.contains(" resumed>") {
            let prefix = pending
                .remove(pid)
                .ok_or_else(|| format!("unmatched resume: {line}"))?;
            let (_, suffix) = rest
                .split_once(" resumed>")
                .ok_or("missing resume marker")?;
            events.push(format!("{prefix}{suffix}"));
        } else {
            events.push(line.to_string());
        }
    }
    if !pending.is_empty() {
        return Err("unfinished syscall at end of trace".into());
    }
    Ok(events)
}

fn witness(path: &Path, cli: &str, owned: bool) -> Result<(), Box<dyn std::error::Error>> {
    let trace = std::fs::read_to_string(path)?;
    let events = events(&trace)?;
    let marker = format!("execve(\"{cli}\"");
    let starts: Vec<_> = events
        .iter()
        .enumerate()
        .filter(|(_, line)| line.contains(&marker) && line.ends_with("= 0"))
        .collect();
    assert_eq!(
        starts.len(),
        1,
        "one actual built CLI invocation per selected case"
    );
    let (start, invocation) = starts[0];
    let pid = invocation
        .split_whitespace()
        .next()
        .ok_or("missing CLI PID")?;
    let owned_root = if owned {
        let cwd_event = events
            .iter()
            .find(|line| {
                line.starts_with(&format!("{pid} "))
                    && line.contains("chdir(")
                    && line.contains("/decoy\"")
                    && line.ends_with("= 0")
            })
            .ok_or("actual CLI did not enter its foreign decoy CWD")?;
        let (_, cwd_text) = cwd_event.split_once("chdir(\"").ok_or("missing CWD path")?;
        let (cwd, _) = cwd_text.split_once('"').ok_or("unterminated CWD path")?;
        let scratch = cwd
            .strip_suffix("/decoy")
            .ok_or("CWD is not the declared decoy")?;
        assert!(
            scratch.contains("/ripr-sample-output-"),
            "unexpected scratch: {scratch}"
        );
        Some(format!("{scratch}/owned/"))
    } else {
        None
    };
    let mut reads = BTreeSet::new();
    for line in &events[start..] {
        if !line.contains("openat(") || !line.contains("O_RDONLY") {
            continue;
        }
        let Some((_, returned)) = line.rsplit_once(" = ") else {
            continue;
        };
        let Some((fd, rest)) = returned.split_once('<') else {
            continue;
        };
        if fd.parse::<u32>().is_err() {
            continue;
        }
        let Some((path, _)) = rest.split_once('>') else {
            continue;
        };
        if !path.ends_with(".rs") {
            continue;
        }
        let path = if let Some(root) = &owned_root {
            let suffix = path
                .strip_prefix(root)
                .ok_or_else(|| format!("Rust read outside selected fixture {root}: {path}"))?;
            format!("$OWNED/{suffix}")
        } else {
            path.to_string()
        };
        reads.insert(path);
    }
    if owned {
        assert_eq!(
            reads,
            BTreeSet::from([
                "$OWNED/crates/ripr/examples/sample/src/lib.rs".to_string(),
                "$OWNED/crates/ripr/examples/sample/tests/pricing.rs".to_string(),
            ]),
            "exact admitted source and test set, including worker-thread reads"
        );
    } else {
        assert!(
            reads.len() > 2,
            "baseline must observe its full-checkout corpus"
        )
    }
    let processes = events
        .iter()
        .filter(|line| line.contains("execve(") && line.ends_with("= 0"))
        .count();
    println!(
        "{}: PASS; Rust inputs={}; successful execs={processes}; CLI=1",
        path.display(),
        reads.len()
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [mode, cli, traces @ ..] = args.as_slice() else {
        return Err("usage: check_reads <base|owned> <absolute-cli> <trace>...".into());
    };
    assert!(matches!(mode.as_str(), "base" | "owned"));
    assert!(!traces.is_empty());
    for trace in traces {
        witness(Path::new(trace), cli, mode == "owned")?
    }
    Ok(())
}
