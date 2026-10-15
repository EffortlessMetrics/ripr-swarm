//! Source contract for the hosted cache writer opportunity (#7827).
//!
//! This does not observe a save, a hit, or cross-ref readability. When retiring
//! main-push qualification, replace this temporary contract with the reviewed
//! selected-route lifecycle proof; do not retain full qualification just to warm
//! a cache. The archive-boundary contract (#7685) remains independent.
const CALLER: &str = include_str!("../../../.github/workflows/routed-rust.yml");
const PRODUCER: &str = include_str!("../../../.github/workflows/rust-gates.yml");

fn top_block<'a>(source: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("\n{name}:\n");
    let (_, tail) = source.split_once(&marker)?;
    let end = tail
        .lines()
        .scan(0, |offset, line| {
            let start = *offset;
            *offset += line.len() + 1;
            Some((start, line))
        })
        .find(|(_, line)| !line.is_empty() && !line.starts_with(' '))
        .map_or(tail.len(), |(offset, _)| offset);
    Some(&tail[..end])
}

fn job<'a>(source: &'a str, name: &str) -> Option<&'a str> {
    let jobs = top_block(source, "jobs")?;
    let marker = format!("  {name}:\n");
    let (_, tail) = jobs.split_once(&marker)?;
    // Search line boundaries rather than treating the first nested field as
    // the next job (the common case in reusable-workflow callers).
    let mut offset = 0;
    for line in tail.split_inclusive('\n') {
        if line.starts_with("  ") && !line.starts_with("   ") && !line.trim().is_empty() {
            return Some(&tail[..offset]);
        }
        offset += line.len();
    }
    Some(tail)
}

fn continuity_findings(caller: &str, producer: &str) -> Vec<&'static str> {
    let mut findings = Vec::new();
    let events = top_block(caller, "on").unwrap_or("");
    let push = events
        .split_once("  push:\n")
        .map(|(_, tail)| tail.split("\n  ").next().unwrap_or(""));
    if push != Some("    branches: [main, master]") {
        findings.push("main-only hosted cache loses its automatic writer opportunity; reconcile #7827 before retirement");
    }
    let hosted = job(caller, "rust-github").unwrap_or("");
    for required in [
        "    uses: ./.github/workflows/rust-gates.yml\n",
        "      use-scratch: false\n",
    ] {
        if !hosted.contains(required) {
            findings
                .push("hosted caller/cache producer binding changed; requalify writer eligibility");
        }
    }
    let step = producer
        .split("\n      - ")
        .find(|block| block.starts_with("name: Restore hosted Rust cache\n"))
        .unwrap_or("");
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
    assert!(retired.contains("types: [ready_for_review]"));
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
