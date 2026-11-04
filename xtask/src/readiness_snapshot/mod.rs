mod build;
mod command;
mod identity;
mod model;
mod rehearsal;
mod rehearsal_plan;
mod render;

pub(crate) use command::run;
pub(crate) use rehearsal_plan::run as run_rehearsal;

pub(crate) fn validate_readiness_snapshot_fixture_corpus(violations: &mut Vec<String>) {
    use std::path::Path;

    let root = Path::new("fixtures/readiness-snapshot");
    let spec = root.join("SPEC.md");
    if !spec.is_file() {
        violations.push("fixtures/readiness-snapshot: missing SPEC.md".to_string());
    }
    for name in [
        "rehearsal-receipt.complete.json",
        "rehearsal-receipt.wrong-sha.json",
        "rehearsal-receipt.supply-chain.json",
        "rehearsal-receipt.plan.json",
    ] {
        let path = root.join(name);
        if !path.is_file() {
            violations.push(format!("fixtures/readiness-snapshot: missing `{name}`"));
            continue;
        }
        let body = match std::fs::read_to_string(&path) {
            Ok(body) => body,
            Err(error) => {
                violations.push(format!("fixtures/readiness-snapshot/{name}: {error}"));
                continue;
            }
        };
        if let Err(error) = rehearsal::parse_rehearsal_receipt(&body) {
            violations.push(format!("fixtures/readiness-snapshot/{name}: {error}"));
        }
    }
}

#[cfg(test)]
mod tests;
