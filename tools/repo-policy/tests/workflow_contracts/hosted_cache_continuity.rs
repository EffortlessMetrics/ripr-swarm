//! Hosted cache lifecycle after main-push retirement (#7827, #14500).
//!
//! Existing default-branch entries accelerate compatible reads until mismatch
//! or eviction. There is no ordinary automatic writer. An already-selected
//! manual main qualification may save; cache absence never skips compilation or
//! tests. This source contract does not claim a hit, persistence or cold timing.
const CALLER: &str = include_str!("../../../../.github/workflows/routed-rust.yml");
const GATES: &str = include_str!("../../../../.github/workflows/rust-gates.yml");
const SHARDS: &str = include_str!("../../../../.github/workflows/rust-test-shards.yml");

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

fn step<'a>(source: &'a str, name: &str) -> &'a str {
    source
        .split("\n      - ")
        .find(|candidate| candidate.starts_with(&format!("name: {name}\n")))
        .unwrap_or("")
}

fn findings(caller: &str, gates: &str, shards: &str) -> Vec<&'static str> {
    let mut findings = Vec::new();
    let events = block(caller, "on", 0).unwrap_or("");
    if block(events, "push", 2).is_some() || block(events, "schedule", 2).is_some() {
        findings.push("ordinary push or scheduled qualification cannot be a cache warmer");
    }
    for event in ["pull_request", "merge_group", "workflow_dispatch"] {
        if block(events, event, 2).is_none() {
            findings.push("retained H/G/manual qualification is missing");
        }
    }
    for (job, producer) in [
        ("rust-github", "rust-gates"),
        ("rust-github-tests", "rust-test-shards"),
    ] {
        let hosted = block(caller, "jobs", 0)
            .and_then(|jobs| block(jobs, job, 2))
            .unwrap_or("");
        if !hosted.contains(&format!("    uses: ./.github/workflows/{producer}.yml\n")) {
            findings.push("selected hosted producer binding changed");
        }
    }
    for (source, key, guard, scratch) in [
        (
            gates,
            "ripr-swarm-rust-small-${{ runner.os }}",
            "${{ github.ref == 'refs/heads/main' }}",
            true,
        ),
        (
            shards,
            "ripr-swarm-rust-shards-${{ runner.os }}",
            "${{ github.ref == 'refs/heads/main' && matrix.shard == 1 }}",
            false,
        ),
    ] {
        let caches: Vec<_> = source
            .split("\n      - ")
            .filter(|step| {
                step.lines()
                    .any(|line| line.trim().starts_with("uses: Swatinem/rust-cache@"))
            })
            .collect();
        if caches.len() != 1 {
            findings.push("each selected hosted producer has exactly one cache domain");
            continue;
        }
        let cache = caches[0];
        for expected in [
            "uses: Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0".to_string(),
            format!("          shared-key: {key}\n"),
            format!("          save-if: {guard}\n"),
        ] {
            if !cache.contains(&expected) {
                findings.push("cache key/version or default-branch-only save authority changed");
            }
        }
        if scratch && !cache.contains("        if: ${{ !inputs.use-scratch }}\n") {
            findings.push("hosted cache must not enter owned scratch storage");
        }
        if cache.contains("cache-workspace-crates:")
            || cache.contains("cache-targets:")
            || cache.contains("workspaces:")
        {
            findings.push("cache storage domain requires separate contamination review");
        }
    }
    for (source, name, command) in [
        (
            gates,
            "Repository policy preflight",
            "run: cargo xtask ci-policy",
        ),
        (
            gates,
            "Strict all-features ripr Clippy",
            "run: cargo clippy -p ripr --all-targets --all-features -- -D warnings",
        ),
        (
            gates,
            "Strict workspace Clippy",
            "run: cargo clippy --workspace --all-targets -- -D warnings",
        ),
        (
            shards,
            "Build the test binaries one memory-heavy crate at a time",
            "build_phase workspace cargo test --no-run --locked --workspace",
        ),
        (
            shards,
            "Required Rust tests (shard ${{ matrix.shard }} of 2)",
            "          cargo nextest run --workspace --profile ci --partition hash:${{ matrix.shard }}/2\n",
        ),
    ] {
        let selected = step(source, name);
        if !selected.contains(command)
            || selected.lines().any(|line| {
                line.starts_with("        if:") || line.starts_with("        continue-on-error:")
            })
        {
            findings.push("cache hits or misses must not skip mandatory recomputation/proof");
        }
    }
    findings
}

#[test]
fn current_source_restores_opportunistically_and_recomputes_required_proof() {
    assert_eq!(findings(CALLER, GATES, SHARDS), Vec::<&str>::new());
}

#[test]
fn ordinary_warmer_cannot_return() {
    for event in [
        "  push:\n    branches: [main, master]\n",
        "  schedule:\n    - cron: '0 * * * *'\n",
    ] {
        let changed = CALLER.replacen("on:\n", &format!("on:\n{event}"), 1);
        assert!(
            findings(&changed, GATES, SHARDS)
                .iter()
                .any(|f| f.contains("cache warmer"))
        );
    }
}

#[test]
fn neither_gate_nor_shard_can_broaden_write_authority() {
    for (gates, shards) in [
        (
            GATES.replace(
                "save-if: ${{ github.ref == 'refs/heads/main' }}",
                "save-if: true",
            ),
            SHARDS.to_owned(),
        ),
        (
            GATES.to_owned(),
            SHARDS.replace(
                "save-if: ${{ github.ref == 'refs/heads/main' && matrix.shard == 1 }}",
                "save-if: ${{ github.event_name == 'merge_group' }}",
            ),
        ),
    ] {
        assert!(!findings(CALLER, &gates, &shards).is_empty());
    }
}

#[test]
fn missing_changed_or_duplicate_cache_domains_refuse() {
    for source in [GATES, SHARDS] {
        for changed in [
            source.replace("uses: Swatinem/rust-cache@", "uses: other/cache@"),
            source.replace(
                "          shared-key:",
                "          cache-workspace-crates: true\n          shared-key:",
            ),
            source.replace(
                "          shared-key:",
                "          workspaces: '. -> shared'\n          shared-key:",
            ),
            format!("{source}\n      - uses: Swatinem/rust-cache@other\n"),
        ] {
            let actual = if source == GATES {
                findings(CALLER, &changed, SHARDS)
            } else {
                findings(CALLER, GATES, &changed)
            };
            assert!(!actual.is_empty());
        }
    }
}

#[test]
fn hit_or_miss_never_substitutes_for_compilation_or_tests() {
    for (source, name) in [
        (GATES, "Repository policy preflight"),
        (GATES, "Strict workspace Clippy"),
        (
            SHARDS,
            "Build the test binaries one memory-heavy crate at a time",
        ),
        (
            SHARDS,
            "Required Rust tests (shard ${{ matrix.shard }} of 2)",
        ),
    ] {
        let needle = format!("      - name: {name}\n");
        let changed = source.replace(
            &needle,
            &format!("{needle}        if: steps.cache.outputs.cache-hit != 'true'\n"),
        );
        assert_ne!(changed, source);
        let actual = if source == GATES {
            findings(CALLER, &changed, SHARDS)
        } else {
            findings(CALLER, GATES, &changed)
        };
        assert!(
            actual
                .iter()
                .any(|finding| finding.contains("mandatory recomputation"))
        );
    }
}

#[test]
fn absent_proof_or_rebound_caller_is_not_cache_continuity() {
    let no_build = SHARDS.replace(
        "build_phase workspace cargo test --no-run --locked --workspace",
        "true",
    );
    assert!(!findings(CALLER, GATES, &no_build).is_empty());
    for producer in ["rust-gates", "rust-test-shards"] {
        let changed = CALLER.replace(
            &format!("uses: ./.github/workflows/{producer}.yml"),
            "uses: ./.github/workflows/other.yml",
        );
        assert!(!findings(&changed, GATES, SHARDS).is_empty());
    }
}
