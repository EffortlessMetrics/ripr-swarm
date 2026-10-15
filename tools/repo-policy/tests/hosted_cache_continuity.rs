//! Source contract for the hosted cache writer opportunity (#7827).
//!
//! This does not observe a save, a hit, or cross-ref readability. When retiring
//! main-push qualification, replace this temporary contract with the reviewed
//! selected-route lifecycle proof; do not retain full qualification just to warm
//! a cache. The archive-boundary contract (#7685) remains independent.
const CALLER: &str = include_str!("../../../.github/workflows/routed-rust.yml");
const PRODUCER: &str = include_str!("../../../.github/workflows/rust-gates.yml");

fn block<'a>(source: &'a str, name: &str, indent: usize) -> Option<&'a str> {
    let marker = format!("{}{name}:", " ".repeat(indent));
    let mut start = 0;
    let tail = source.split_inclusive('\n').find_map(|line| {
        start += line.len();
        (line.trim_end_matches(['\n', '\r']) == marker).then(|| &source[start..])
    })?;
    let mut offset = 0;
    for line in tail.split_inclusive('\n') {
        if !line.trim().is_empty()
            && line.bytes().take_while(|byte| *byte == b' ').count() <= indent
        {
            return Some(&tail[..offset]);
        }
        offset += line.len();
    }
    Some(tail)
}
// The staged PR path may skip admission on retained main/manual qualification.
// Pin the reviewed status override rather than treating a restored trigger as
// reachability through a PR-only prerequisite. This is still source-shape proof.
const STAGED_PREREQUISITE_GUARD: &str = "!cancelled() && ((github.event_name == 'push' && (github.ref == 'refs/heads/main' || github.ref == 'refs/heads/master')) || github.event_name == 'workflow_dispatch' || (github.event_name == 'pull_request' && needs.admission.result == 'success' && needs.admission.outputs.execute == 'true'))";

fn folded_guard(job: &str) -> Option<String> {
    let header = job.split("    steps:\n").next().unwrap_or("");
    let mut lines = header.lines();
    lines.find(|line| *line == "    if: >-")?;
    Some(
        lines
            .take_while(|line| line.starts_with("      "))
            .map(str::trim)
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn continuity_findings(caller: &str, producer: &str) -> Vec<&'static str> {
    let mut findings = Vec::new();
    let events = block(caller, "on", 0).unwrap_or("");
    let push = block(events, "push", 2);
    if push.map(str::trim) != Some("branches: [main, master]") {
        findings.push("main-only hosted cache loses its automatic writer opportunity; reconcile #7827 before retirement");
    }
    let jobs = block(caller, "jobs", 0).unwrap_or("");
    let admission = block(jobs, "admission", 2);
    for prerequisite in ["route", "detect-docs-only"] {
        let job = block(jobs, prerequisite, 2).unwrap_or("");
        let header = job.split("    steps:\n").next().unwrap_or("");
        let reviewed = if let Some(admission) = admission {
            admission
                .split("    steps:\n")
                .next()
                .unwrap_or("")
                .contains("    if: github.event_name == 'pull_request'\n")
                && header.contains("    needs: [admission]\n")
                && folded_guard(job).as_deref() == Some(STAGED_PREREQUISITE_GUARD)
        } else {
            !job.is_empty()
                && !header
                    .lines()
                    .any(|line| line.starts_with("    if:") || line.starts_with("    needs:"))
        };
        if !reviewed {
            findings.push("main writer prerequisite admission changed; restored triggers alone are not continuity");
        }
    }
    let hosted = block(jobs, "rust-github", 2).unwrap_or("");
    for required in [
        "    needs: [route, detect-docs-only, rust-cx43, rust-cpx42, rust-cx53]\n",
        "    uses: ./.github/workflows/rust-gates.yml\n",
        "      use-scratch: false\n",
    ] {
        if !hosted.contains(required) {
            findings
                .push("hosted caller/cache producer binding changed; requalify writer eligibility");
        }
    }
    let guard = folded_guard(hosted);
    let expected_guard = if admission.is_some() {
        "!cancelled() && needs.route.result == 'success' && needs.detect-docs-only.result == 'success' && needs.detect-docs-only.outputs.docs_only != 'true' && (needs.route.outputs.router_target == 'github' || (needs.route.outputs.router_target == 'cx43' && needs.rust-cx43.outputs.scratch_status == 'tempfail') || (needs.route.outputs.router_target == 'cpx42' && needs.rust-cpx42.outputs.scratch_status == 'tempfail') || (needs.route.outputs.router_target == 'cx53' && needs.rust-cx53.outputs.scratch_status == 'tempfail'))"
    } else {
        "!cancelled() && needs.detect-docs-only.result == 'success' && needs.detect-docs-only.outputs.docs_only != 'true' && (needs.route.outputs.router_target == 'github' || (needs.route.outputs.router_target == 'cx43' && needs.rust-cx43.outputs.scratch_status == 'tempfail') || (needs.route.outputs.router_target == 'cpx42' && needs.rust-cpx42.outputs.scratch_status == 'tempfail') || (needs.route.outputs.router_target == 'cx53' && needs.rust-cx53.outputs.scratch_status == 'tempfail'))"
    };
    if guard.as_deref() != Some(expected_guard) {
        findings.push("hosted job eligibility changed; requalify the main writer opportunity");
    }
    let producer_job = block(producer, "jobs", 0)
        .and_then(|jobs| block(jobs, "rust-gates", 2))
        .unwrap_or("");
    let header = producer_job.split("    steps:\n").next().unwrap_or("");
    if producer_job.is_empty()
        || header
            .lines()
            .any(|line| line.starts_with("    if:") || line.starts_with("    needs:"))
    {
        findings.push("producer job admission changed; requalify the main writer opportunity");
    }
    let cache_steps: Vec<_> = producer_job
        .split("\n      - ")
        .filter(|step| {
            step.lines()
                .any(|line| line.trim().starts_with("uses: Swatinem/rust-cache@"))
        })
        .collect();
    if cache_steps.len() != 1 {
        findings.push("producer cache action set changed; review every cache write authority");
    }
    let step = cache_steps.first().copied().unwrap_or("");
    if !step.starts_with("name: Restore hosted Rust cache\n") {
        findings.push("declared hosted producer is missing from its job");
    }
    for required in [
        "        if: ${{ !inputs.use-scratch }}\n",
        "        uses: Swatinem/rust-cache@v2\n",
        "          save-if: ${{ github.ref == 'refs/heads/main' }}\n",
    ] {
        if !step.contains(required) {
            findings.push("main-only cache write authority changed; review ref/trust scope instead of enabling PR saves");
        }
    }
    findings
}

#[test]
fn current_source_preserves_a_main_scope_writer_opportunity() {
    assert_eq!(continuity_findings(CALLER, PRODUCER), Vec::<&str>::new());
}

#[test]
fn retiring_main_push_is_not_cache_continuity() {
    let retired = CALLER.replace("  push:\n    branches: [main, master]\n", "");
    assert_ne!(retired, CALLER, "control must remove the real trigger");
    let findings = continuity_findings(&retired, PRODUCER);
    assert!(
        findings
            .iter()
            .any(|finding| finding.contains("loses its automatic writer")),
        "{findings:?}"
    );
    // Dispatch still exists, but an available manual run is not an automatic
    // lifecycle and a pre-merge ref cannot satisfy the producer's save guard.
    assert!(retired.contains("  workflow_dispatch:"));
    let pr_event = block(block(CALLER, "on", 0).unwrap_or(""), "pull_request", 2).unwrap_or("");
    assert!(!pr_event.is_empty(), "control must retain a real PR event");
    assert_eq!(
        block(block(&retired, "on", 0).unwrap_or(""), "pull_request", 2),
        Some(pr_event),
        "removing main push must leave the actual PR request unchanged"
    );
}

#[test]
fn broadening_saves_is_not_a_trusted_default_scope_writer() {
    let broadened = PRODUCER.replace(
        "save-if: ${{ github.ref == 'refs/heads/main' }}",
        "save-if: true",
    );
    assert_ne!(broadened, PRODUCER);
    assert!(
        continuity_findings(CALLER, &broadened)
            .iter()
            .any(|finding| finding.contains("write authority changed"))
    );
}

#[test]
fn missing_or_rebound_producer_is_not_a_writer_opportunity() {
    for changed in [
        CALLER.replace(
            "uses: ./.github/workflows/rust-gates.yml",
            "uses: ./.github/workflows/other.yml",
        ),
        CALLER.replace("use-scratch: false", "use-scratch: true"),
    ] {
        assert_ne!(changed, CALLER);
        assert!(
            continuity_findings(&changed, PRODUCER)
                .iter()
                .any(|finding| finding.contains("binding changed"))
        );
    }
    assert!(!continuity_findings("", PRODUCER).is_empty());
    assert!(!continuity_findings(CALLER, "").is_empty());
}

#[test]
fn disabled_hosted_job_or_filtered_push_is_not_writer_continuity() {
    // Address this exact job, rather than whichever multiline guard occurs first.
    let hosted = block(CALLER, "jobs", 0)
        .and_then(|jobs| block(jobs, "rust-github", 2))
        .unwrap_or("");
    let disabled = CALLER.replace(
        hosted,
        &hosted.replace("    if: >-\n", "    if: false\n    # former guard: >-\n"),
    );
    assert_ne!(disabled, CALLER);
    assert!(
        continuity_findings(&disabled, PRODUCER)
            .iter()
            .any(|finding| finding.contains("job eligibility changed"))
    );
    let filtered = CALLER.replace(
        "    branches: [main, master]\n",
        "    branches: [main, master]\n    paths: ['cache-writer-only/**']\n",
    );
    assert_ne!(filtered, CALLER);
    assert!(
        continuity_findings(&filtered, PRODUCER)
            .iter()
            .any(|finding| finding.contains("loses its automatic writer"))
    );
}

#[test]
fn missing_caller_dependencies_cannot_supply_guard_outputs() {
    for needs in [
        "needs: [detect-docs-only, rust-cx43, rust-cpx42, rust-cx53]",
        "needs: [route, rust-cx43, rust-cpx42, rust-cx53]",
    ] {
        let changed = CALLER.replace(
            "needs: [route, detect-docs-only, rust-cx43, rust-cpx42, rust-cx53]",
            needs,
        );
        assert_ne!(changed, CALLER);
        assert!(!continuity_findings(&changed, PRODUCER).is_empty());
    }
}

#[test]
fn disabled_producer_or_additional_cache_is_not_writer_continuity() {
    for changed in [
        PRODUCER.replace("  rust-gates:\n", "  rust-gates:\n    if: false\n"),
        PRODUCER.replace("      - name: Restore hosted Rust cache\n", "      - uses: Swatinem/rust-cache@v2\n        with:\n          save-if: true\n      - name: Restore hosted Rust cache\n"),
    ] {
        assert_ne!(changed, PRODUCER);
        assert!(!continuity_findings(CALLER, &changed).is_empty());
    }
}

#[test]
fn declared_triggers_do_not_bypass_pr_only_prerequisites() {
    let jobs = block(CALLER, "jobs", 0).unwrap_or("");
    assert!(!jobs.is_empty());
    for prerequisite in ["route", "detect-docs-only"] {
        let job = block(jobs, prerequisite, 2).unwrap_or("");
        assert!(!job.is_empty());
        let mut in_guard = false;
        let ungated = job
            .lines()
            .filter(|line| {
                if line.starts_with("    if:") {
                    in_guard = true;
                    return false;
                }
                if in_guard && line.starts_with("      ") {
                    return false;
                }
                in_guard = false;
                !line.starts_with("    needs:")
            })
            .collect::<Vec<_>>()
            .join("\n");
        let blocked = format!(
            "    needs: [admission]\n    if: needs.admission.outputs.execute == 'true'\n{ungated}\n"
        );
        let mut changed = CALLER.replace(job, &blocked);
        if block(jobs, "admission", 2).is_none() {
            changed = changed.replacen(
                "jobs:\n",
                "jobs:\n  admission:\n    if: github.event_name == 'pull_request'\n    runs-on: ubuntu-latest\n    outputs:\n      execute: ${{ steps.admit.outputs.execute }}\n    steps:\n      - id: admit\n        run: echo execute=true >> $GITHUB_OUTPUT\n",
                1,
            );
        }
        if block(block(&changed, "on", 0).unwrap_or(""), "push", 2).is_none() {
            changed = changed.replacen(
                "on:\n",
                "on:\n  push:\n    branches: [main, master]\n  workflow_dispatch:\n",
                1,
            );
        }
        assert_ne!(changed, CALLER);
        assert_eq!(
            block(block(&changed, "on", 0).unwrap_or(""), "push", 2).map(str::trim),
            Some("branches: [main, master]")
        );
        assert!(
            continuity_findings(&changed, PRODUCER)
                .iter()
                .any(|finding| finding.contains("prerequisite admission changed"))
        );
    }
}
