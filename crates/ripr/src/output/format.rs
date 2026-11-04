/// Output renderer selection for `ripr` reports.
///
/// Most automation should prefer [`OutputFormat::Json`] for stable
/// machine-readable data. Badge and repo-inventory formats exist for specific
/// downstream integrations and may require additional artifacts.
use crate::analysis::seam_cache::ClassifiedCacheFamily;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    /// Bounded human-readable triage report.
    Human,
    /// Exhaustive human-readable report with every finding body.
    HumanFull,
    /// Versioned JSON report for automation.
    Json,
    /// Unbounded `json` check findings with the producer `artifact` identity
    /// envelope (ADR 0020 amendment), so a gap-keyed verify can reject a
    /// fabricated, edited or incomplete snapshot.
    CheckFindingsJson,
    /// GitHub annotation output suitable for CI logs.
    Github,
    /// SARIF 2.1.0 report for diff-scoped static exposure Findings.
    Sarif,
    /// Native `ripr` badge JSON (snake_case wire shape with full counts,
    /// reason counts, and policy). Consumed by tools and CI artifacts.
    BadgeJson,
    /// Shields-compatible projection for the `ripr` badge: exactly four
    /// top-level fields (`schemaVersion`, `label`, `message`, `color`).
    BadgeShields,
    /// Native `ripr+` badge JSON. Sums unsuppressed exposure gaps and
    /// unsuppressed actionable test-efficiency findings, excluding
    /// declared intent. When `target/ripr/reports/test-efficiency.json`
    /// is missing, renders a neutral badge-generator-safe response.
    BadgePlusJson,
    /// Shields-compatible projection for the `ripr+` badge.
    BadgePlusShields,
    /// Repo-scoped native `ripr` badge JSON. Renders unresolved actionable
    /// canonical repair items rather than diff-scoped `Finding` counts or
    /// seam-native inventory. Carries `scope: "repo"` and
    /// `basis: "canonical_actionable_gap"` so README/store endpoints can
    /// distinguish public repair signal from PR/diff and inventory artifacts.
    RepoBadgeJson,
    /// Repo-scoped Shields projection for the `ripr` badge. Same four
    /// fields as the diff-scoped Shields shape; native-only fields like
    /// `scope` and `basis` do not leak into Shields.
    RepoBadgeShields,
    /// Repo-scoped native `ripr+` badge JSON. Uses the same
    /// test-efficiency report as `BadgePlusJson` when present, but raw
    /// test-efficiency debt does not move the repo headline until it is
    /// lifted into the same actionable repair / verify / receipt model as
    /// canonical gaps.
    RepoBadgePlusJson,
    /// Repo-scoped Shields projection for the `ripr+` badge.
    RepoBadgePlusShields,
    /// Repo seam inventory rendered as JSON. Walks production Rust
    /// files and emits `RepoSeam` records per RIPR-SPEC-0005. Schema
    /// version is documented in `docs/OUTPUT_SCHEMA.md` under
    /// `repo-seams.json`. Independent of the diff-scoped `Findings`
    /// pipeline.
    RepoSeamsJson,
    /// Repo seam inventory rendered as Markdown for human review.
    RepoSeamsMd,
    /// Classified seam inventory rendered as a repo exposure JSON
    /// report. Adds per-seam grip class and per-class metrics on top
    /// of the seam inventory. Schema in `docs/OUTPUT_SCHEMA.md` under
    /// `repo-exposure.json`.
    RepoExposureJson,
    /// Bounded repo exposure summary rendered as JSON. Emits aggregate
    /// canonical actionable gap counts, reason breakdowns, and a capped
    /// top-file summary without per-seam evidence payloads.
    RepoExposureSummaryJson,
    /// Repo exposure report rendered as Markdown for human review.
    RepoExposureMd,
    /// SARIF 2.1.0 report for repo-scoped classified seam evidence.
    RepoSarif,
    /// Agent-ready seam packets per RIPR-SPEC-0005 - one
    /// `write_targeted_test` packet per headline-eligible classified
    /// seam, plus conservative `inspect_static_limitation` packets for
    /// opaque seams. Schema 0.3 in `docs/OUTPUT_SCHEMA.md` section "Agent
    /// Seam Packets". Strongly-gripped, intentional, and suppressed
    /// seams emit no packet.
    AgentSeamPacketsJson,
    /// Compact, legend-free, line-oriented diff-scoped report for coding
    /// agents (#11580): verdict counts, bounded top gaps with `file:line`,
    /// missing discriminator, strongest related test and finding id, an
    /// explicit `done=`/`stop=` terminal line, and the shared `next=`
    /// command. A projection of the human triage; carries no new verdict.
    Agent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OutputFormatSpec {
    format: OutputFormat,
    cli_names: &'static [&'static str],
    is_repo_seam_inventory: bool,
    /// `true` for the full-repo audit-path formats (the help's
    /// "Repo-scope (full-repo analysis)" group plus
    /// `agent-seam-packets-json`): every invocation walks and classifies the
    /// whole Rust corpus, so the CLI discloses the expected cost class at
    /// invocation time (#4945). The seam-native repo badge formats joined
    /// this class when their `canonical_actionable_gap` basis moved onto the
    /// full classified inventory so it agrees with repo-exposure (#5261);
    /// the repo badge-plus measured path renders that same walk when a
    /// test-efficiency report exists (#5263 review), so it discloses too.
    is_full_repo_analysis: bool,
    /// `true` only when warm reruns of the format's walk can hit a
    /// seam-facts cache: the classified and compact-classified inventory
    /// paths (`seam_inventory.rs` `RepoSeamFactCache::at` /
    /// `::at_compact_classified`). The raw `repo-seams-*` walks rebuild the
    /// corpus index on every invocation and read no cache, so their
    /// disclosure must not promise cache-backed warm reruns (#4945 review).
    is_seam_fact_cache_backed: bool,
    /// Which seam-facts cache family the format's walk reads, when any
    /// (#12505). The invocation-time disclosure probes exactly this
    /// family's entries before claiming warm reruns; `None` on formats
    /// that read no seam-facts cache. Set together with
    /// `is_seam_fact_cache_backed` (parity pinned by the disclosure
    /// coverage test) and kept in sync with the inventory acquisition
    /// each renderer performs.
    seam_fact_cache_family: Option<ClassifiedCacheFamily>,
}

const FORMAT_SPECS: &[OutputFormatSpec] = &[
    OutputFormatSpec {
        format: OutputFormat::Human,
        cli_names: &["human", "text"],
        is_full_repo_analysis: false,
        is_seam_fact_cache_backed: false,
        seam_fact_cache_family: None,
        is_repo_seam_inventory: false,
    },
    OutputFormatSpec {
        format: OutputFormat::HumanFull,
        cli_names: &["human-full", "text-full"],
        is_full_repo_analysis: false,
        is_seam_fact_cache_backed: false,
        seam_fact_cache_family: None,
        is_repo_seam_inventory: false,
    },
    OutputFormatSpec {
        format: OutputFormat::Json,
        cli_names: &["json"],
        is_full_repo_analysis: false,
        is_seam_fact_cache_backed: false,
        seam_fact_cache_family: None,
        is_repo_seam_inventory: false,
    },
    OutputFormatSpec {
        format: OutputFormat::CheckFindingsJson,
        cli_names: &["check-findings-json"],
        is_full_repo_analysis: false,
        is_seam_fact_cache_backed: false,
        seam_fact_cache_family: None,
        is_repo_seam_inventory: false,
    },
    OutputFormatSpec {
        format: OutputFormat::Github,
        cli_names: &["github"],
        is_full_repo_analysis: false,
        is_seam_fact_cache_backed: false,
        seam_fact_cache_family: None,
        is_repo_seam_inventory: false,
    },
    OutputFormatSpec {
        format: OutputFormat::Agent,
        cli_names: &["agent"],
        is_full_repo_analysis: false,
        is_seam_fact_cache_backed: false,
        seam_fact_cache_family: None,
        is_repo_seam_inventory: false,
    },
    OutputFormatSpec {
        format: OutputFormat::Sarif,
        cli_names: &["sarif"],
        is_full_repo_analysis: false,
        is_seam_fact_cache_backed: false,
        seam_fact_cache_family: None,
        is_repo_seam_inventory: false,
    },
    OutputFormatSpec {
        format: OutputFormat::BadgeJson,
        cli_names: &["badge-json"],
        is_full_repo_analysis: false,
        is_seam_fact_cache_backed: false,
        seam_fact_cache_family: None,
        is_repo_seam_inventory: false,
    },
    OutputFormatSpec {
        format: OutputFormat::BadgeShields,
        cli_names: &["badge-shields"],
        is_full_repo_analysis: false,
        is_seam_fact_cache_backed: false,
        seam_fact_cache_family: None,
        is_repo_seam_inventory: false,
    },
    OutputFormatSpec {
        format: OutputFormat::BadgePlusJson,
        cli_names: &["badge-plus-json"],
        is_full_repo_analysis: false,
        is_seam_fact_cache_backed: false,
        seam_fact_cache_family: None,
        is_repo_seam_inventory: false,
    },
    OutputFormatSpec {
        format: OutputFormat::BadgePlusShields,
        cli_names: &["badge-plus-shields"],
        is_full_repo_analysis: false,
        is_seam_fact_cache_backed: false,
        seam_fact_cache_family: None,
        is_repo_seam_inventory: false,
    },
    OutputFormatSpec {
        format: OutputFormat::RepoBadgeJson,
        cli_names: &["repo-badge-json"],
        // #5261: the canonical_actionable_gap basis now derives from the
        // full classified inventory so the badge cannot contradict
        // repo-exposure on the same tree; the cost disclosure follows.
        is_full_repo_analysis: true,
        is_seam_fact_cache_backed: true,
        seam_fact_cache_family: Some(ClassifiedCacheFamily::SeamFacts),
        is_repo_seam_inventory: true,
    },
    OutputFormatSpec {
        format: OutputFormat::RepoBadgeShields,
        cli_names: &["repo-badge-shields"],
        is_full_repo_analysis: true,
        is_seam_fact_cache_backed: true,
        seam_fact_cache_family: Some(ClassifiedCacheFamily::SeamFacts),
        is_repo_seam_inventory: true,
    },
    OutputFormatSpec {
        format: OutputFormat::RepoBadgePlusJson,
        cli_names: &["repo-badge-plus-json"],
        // #5263 review: the measured repo ripr+ path renders the same full
        // classified walk as repo-badge-json when a test-efficiency report
        // exists, so it discloses the same cost class; the neutral
        // missing-report path stays cheap and merely over-discloses.
        is_full_repo_analysis: true,
        is_seam_fact_cache_backed: true,
        seam_fact_cache_family: Some(ClassifiedCacheFamily::SeamFacts),
        is_repo_seam_inventory: true,
    },
    OutputFormatSpec {
        format: OutputFormat::RepoBadgePlusShields,
        cli_names: &["repo-badge-plus-shields"],
        is_full_repo_analysis: true,
        is_seam_fact_cache_backed: true,
        seam_fact_cache_family: Some(ClassifiedCacheFamily::SeamFacts),
        is_repo_seam_inventory: true,
    },
    OutputFormatSpec {
        format: OutputFormat::RepoSeamsJson,
        cli_names: &["repo-seams-json"],
        is_full_repo_analysis: true,
        is_seam_fact_cache_backed: false,
        seam_fact_cache_family: None,
        is_repo_seam_inventory: true,
    },
    OutputFormatSpec {
        format: OutputFormat::RepoSeamsMd,
        cli_names: &["repo-seams-md"],
        is_full_repo_analysis: true,
        is_seam_fact_cache_backed: false,
        seam_fact_cache_family: None,
        is_repo_seam_inventory: true,
    },
    OutputFormatSpec {
        format: OutputFormat::RepoExposureJson,
        cli_names: &["repo-exposure-json"],
        is_full_repo_analysis: true,
        is_seam_fact_cache_backed: true,
        seam_fact_cache_family: Some(ClassifiedCacheFamily::SeamFacts),
        is_repo_seam_inventory: true,
    },
    OutputFormatSpec {
        format: OutputFormat::RepoExposureSummaryJson,
        cli_names: &["repo-exposure-summary-json"],
        is_full_repo_analysis: true,
        is_seam_fact_cache_backed: true,
        // The compact walk reads only the compact-classified family, which
        // deliberately does not share entries with the classified family
        // (seam_cache.rs), so its disclosure probes the compact layer only.
        seam_fact_cache_family: Some(ClassifiedCacheFamily::CompactClassifiedSeams),
        is_repo_seam_inventory: true,
    },
    OutputFormatSpec {
        format: OutputFormat::RepoExposureMd,
        cli_names: &["repo-exposure-md"],
        is_full_repo_analysis: true,
        is_seam_fact_cache_backed: true,
        seam_fact_cache_family: Some(ClassifiedCacheFamily::SeamFacts),
        is_repo_seam_inventory: true,
    },
    OutputFormatSpec {
        format: OutputFormat::RepoSarif,
        cli_names: &["repo-sarif"],
        is_full_repo_analysis: true,
        is_seam_fact_cache_backed: true,
        seam_fact_cache_family: Some(ClassifiedCacheFamily::SeamFacts),
        is_repo_seam_inventory: true,
    },
    OutputFormatSpec {
        format: OutputFormat::AgentSeamPacketsJson,
        cli_names: &["agent-seam-packets-json"],
        is_full_repo_analysis: true,
        is_seam_fact_cache_backed: true,
        seam_fact_cache_family: Some(ClassifiedCacheFamily::SeamFacts),
        is_repo_seam_inventory: true,
    },
];

impl OutputFormat {
    /// Parses a CLI output format name or alias.
    pub(crate) fn parse_cli_name(value: &str) -> Option<Self> {
        FORMAT_SPECS
            .iter()
            .find_map(|spec| spec.cli_names.contains(&value).then_some(spec.format))
    }

    /// Returns the preferred CLI spelling for this output format.
    pub(crate) fn primary_cli_name(&self) -> &'static str {
        FORMAT_SPECS
            .iter()
            .find(|spec| spec.format == *self)
            .and_then(|spec| spec.cli_names.first().copied())
            .unwrap_or("unknown")
    }

    /// Every CLI name this build accepts for `--format`, in declaration
    /// order.
    ///
    /// Derived from [`FORMAT_SPECS`] so an error message can enumerate the
    /// accepted set without a second list that could drift from the parser.
    pub(crate) fn accepted_cli_names() -> Vec<&'static str> {
        FORMAT_SPECS
            .iter()
            .flat_map(|spec| spec.cli_names.iter().copied())
            .collect()
    }

    /// Returns `true` when the format targets full-repo scope rather than
    /// diff scope.
    ///
    /// Repo-scope formats use full-repo inputs. Native repo badge JSON carries
    /// `scope: "repo"` and public repo badge formats carry
    /// `basis: "canonical_actionable_gap"`. The Shields projection stays
    /// four-field for both scopes.
    pub fn is_repo_scope(&self) -> bool {
        self.is_repo_seam_inventory()
    }

    /// Returns `true` when the format renders repo seam-driven artifacts
    /// that do not consume legacy repo `Finding` output.
    ///
    /// These formats short-circuit legacy repo Finding analysis because they
    /// either walk/classify repo seams directly or render badge summaries from
    /// classified seams. Running legacy repo Finding analysis first would add
    /// cost and then be discarded.
    pub fn is_repo_seam_inventory(&self) -> bool {
        FORMAT_SPECS
            .iter()
            .find(|spec| spec.format == *self)
            .is_some_and(|spec| spec.is_repo_seam_inventory)
    }

    /// Returns `true` when the format runs the full-repo seam analysis walk:
    /// the help's "Repo-scope (full-repo analysis)" group plus
    /// `agent-seam-packets-json` and the seam-native repo badge formats
    /// (#5261). Every such invocation walks and classifies the whole
    /// analyzable Rust corpus, so these are the audit-path surfaces the CLI
    /// must disclose invocation-time cost for (#4945). The repo badge-plus
    /// formats render disk reports only and stay outside this class.
    pub(crate) fn is_full_repo_analysis(&self) -> bool {
        FORMAT_SPECS
            .iter()
            .find(|spec| spec.format == *self)
            .is_some_and(|spec| spec.is_full_repo_analysis)
    }

    /// Returns `true` for formats a program or an agent consumes rather than a
    /// person reading a terminal: the JSON, SARIF and badge documents and the
    /// compact `agent` report (#11598). With one of these on a non-terminal
    /// stderr, `ripr progress:` lines are off by default. Exhaustive on
    /// purpose: a new format must decide.
    pub(crate) fn is_machine_readable(&self) -> bool {
        match self {
            Self::Json
            | Self::CheckFindingsJson
            | Self::Sarif
            | Self::BadgeJson
            | Self::BadgeShields
            | Self::BadgePlusJson
            | Self::BadgePlusShields
            | Self::RepoBadgeJson
            | Self::RepoBadgeShields
            | Self::RepoBadgePlusJson
            | Self::RepoBadgePlusShields
            | Self::RepoSeamsJson
            | Self::RepoExposureJson
            | Self::RepoExposureSummaryJson
            | Self::RepoSarif
            | Self::AgentSeamPacketsJson
            | Self::Agent => true,
            Self::Human
            | Self::HumanFull
            | Self::Github
            | Self::RepoSeamsMd
            | Self::RepoExposureMd => false,
        }
    }

    /// Returns `true` only when warm reruns of the format's walk can hit a
    /// seam-facts cache: the classified and compact-classified inventory
    /// paths. The raw `repo-seams-*` walks rebuild the corpus index on every
    /// invocation and read no cache, so only this group may claim faster
    /// warm reruns (#4945 review).
    pub(crate) fn is_seam_fact_cache_backed(&self) -> bool {
        FORMAT_SPECS
            .iter()
            .find(|spec| spec.format == *self)
            .is_some_and(|spec| spec.is_seam_fact_cache_backed)
    }

    /// Which seam-facts cache family this format's walk reads, when any
    /// (#12505). `None` exactly when the walk reads no seam-facts cache;
    /// the parity with `is_seam_fact_cache_backed` is pinned by the
    /// disclosure coverage test.
    pub(crate) fn seam_fact_cache_family(&self) -> Option<ClassifiedCacheFamily> {
        FORMAT_SPECS
            .iter()
            .find(|spec| spec.format == *self)
            .and_then(|spec| spec.seam_fact_cache_family)
    }

    /// Invocation-time stderr disclosure naming the expected cost class for
    /// full-repo audit-path formats (#4945). One line, emitted before the run
    /// begins; honest to the measured magnitude (release notes: a cold
    /// `repo-exposure-json` run took about 76 minutes on 4 vCPUs, a warm run
    /// with the classified cache 42 seconds), without promising a wall clock.
    /// The warm-rerun clause is honest per format and per cache state: only
    /// the cache-backed classified/compact-classified walks may claim
    /// seam-facts cache reuse, and only when a cache entry already exists in
    /// that walk's own family — a workspace whose cache was never populated
    /// (or whose prior run died before the end-of-run publication) hears the
    /// first-run full-price clause instead (#12505). The raw `repo-seams-*`
    /// walks disclose that every run pays the full walk (#4945 review).
    pub(crate) fn repo_audit_path_disclosure(
        &self,
        workspace_root: &std::path::Path,
    ) -> Option<String> {
        if !self.is_full_repo_analysis() {
            return None;
        }
        let warm_rerun_clause = if self.is_seam_fact_cache_backed() {
            match self.seam_fact_cache_family() {
                Some(family)
                    if crate::analysis::seam_cache::seam_facts_cache_entry_present(
                        workspace_root,
                        family,
                    ) =>
                {
                    "warm reruns reuse the seam-facts cache and are much faster."
                }
                _ => {
                    "no seam-facts cache entry is present yet, so this workspace's first run \
                     pays the full walk."
                }
            }
        } else {
            "raw seam inventory does not read the seam-facts cache, so every run pays the full walk."
        };
        Some(format!(
            "ripr: {} is the full-repo audit path: a cold run analyzes every seam in the \
             workspace and can take minutes on large repositories; {warm_rerun_clause}",
            self.primary_cli_name()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{FORMAT_SPECS, OutputFormat};

    /// #11598: the machine/human split decides whether piped progress is off
    /// by default, so pin both sides by CLI name.
    #[test]
    fn machine_readable_formats_are_the_documents_and_the_agent_report() {
        for (name, machine) in [
            ("json", true),
            ("sarif", true),
            ("badge-json", true),
            ("repo-exposure-json", true),
            ("agent", true),
            ("human", false),
            ("human-full", false),
            ("github", false),
            ("repo-seams-md", false),
        ] {
            let parsed = OutputFormat::parse_cli_name(name);
            assert_eq!(
                parsed.map(|format| format.is_machine_readable()),
                Some(machine),
                "{name}"
            );
        }
    }

    #[test]
    fn parses_human_full_aliases() {
        assert_eq!(
            OutputFormat::parse_cli_name("human-full"),
            Some(OutputFormat::HumanFull)
        );
        assert_eq!(
            OutputFormat::parse_cli_name("text-full"),
            Some(OutputFormat::HumanFull)
        );
    }

    #[test]
    fn human_full_is_not_repo_scope() {
        assert!(!OutputFormat::HumanFull.is_repo_scope());
        assert!(!OutputFormat::HumanFull.is_repo_seam_inventory());
    }

    #[test]
    fn output_format_is_repo_scope_only_for_repo_variants() {
        for spec in FORMAT_SPECS {
            assert_eq!(
                spec.format.is_repo_scope(),
                spec.is_repo_seam_inventory,
                "repo scope should match metadata for {:?}",
                spec.format
            );
        }
    }

    #[test]
    fn repo_artifact_formats_use_repo_seam_short_circuit() {
        for spec in FORMAT_SPECS {
            assert_eq!(
                spec.format.is_repo_seam_inventory(),
                spec.is_repo_seam_inventory,
                "repo seam short-circuit should match metadata for {:?}",
                spec.format
            );
        }
    }

    /// #4945: the invocation-time audit-path cost disclosure covers exactly
    /// the full-repo analysis formats — the help's "Repo-scope
    /// (full-repo analysis)" group, `agent-seam-packets-json`, and (since
    /// #5261 moved their basis onto the full classified inventory) the two
    /// seam-native repo badge formats — and no diff-scoped, badge-plus, or
    /// gap-ledger surface claims minutes it does not charge. The warm-rerun
    /// clause is honest per format and per cache state (#12505): against an
    /// empty workspace, only the raw `repo-seams-*` walks keep a full-walk
    /// claim and the cache-backed formats disclose the first-run full-price
    /// clause; `seam_fact_cache_family` is set exactly on the cache-backed
    /// formats.
    #[test]
    fn repo_format_audit_path_disclosure_covers_exactly_the_full_repo_analysis_group()
    -> Result<(), String> {
        let root = temp_disclosure_root("coverage-group")?;
        for spec in FORMAT_SPECS {
            let disclosure = spec.format.repo_audit_path_disclosure(&root);
            assert_eq!(
                spec.format.is_full_repo_analysis(),
                spec.is_full_repo_analysis,
                "full-repo-analysis predicate should match metadata for {:?}",
                spec.format
            );
            assert_eq!(
                spec.format.is_seam_fact_cache_backed(),
                spec.is_seam_fact_cache_backed,
                "cache-backed predicate should match metadata for {:?}",
                spec.format
            );
            assert_eq!(
                spec.format.seam_fact_cache_family().is_some(),
                spec.is_seam_fact_cache_backed,
                "cache family should be set exactly on cache-backed formats for {:?}",
                spec.format
            );
            if spec.is_full_repo_analysis {
                let disclosure = disclosure
                    .ok_or_else(|| format!("missing disclosure for {:?}", spec.format))?;
                assert!(
                    disclosure.contains("full-repo audit path"),
                    "disclosure must name the audit path: {disclosure}"
                );
                assert!(
                    disclosure.contains("minutes"),
                    "disclosure must name the cold-run cost class: {disclosure}"
                );
                assert!(
                    disclosure.contains(spec.format.primary_cli_name()),
                    "disclosure must name the format: {disclosure}"
                );
                if spec.is_seam_fact_cache_backed {
                    assert!(
                        disclosure.contains(
                            "no seam-facts cache entry is present yet, so this workspace's \
                             first run pays the full walk"
                        ),
                        "cache-backed format on a cache-empty workspace must disclose the \
                         first-run full-price clause: {disclosure}"
                    );
                    assert!(
                        !disclosure.contains("warm reruns reuse"),
                        "cache-backed format must not promise warm reruns without a cache: \
                         {disclosure}"
                    );
                    assert!(
                        !disclosure.contains("every run pays the full walk"),
                        "cache-backed format must not claim an uncached walk: {disclosure}"
                    );
                } else {
                    assert!(
                        disclosure.contains("every run pays the full walk"),
                        "raw seam format must disclose the uncached walk: {disclosure}"
                    );
                    assert!(
                        !disclosure.contains("warm reruns reuse"),
                        "raw seam format must not claim cache-backed warm reruns: {disclosure}"
                    );
                }
            } else {
                assert!(
                    disclosure.is_none(),
                    "non-audit format {:?} must not claim audit-path cost: {disclosure:?}",
                    spec.format
                );
                assert!(
                    !spec.is_seam_fact_cache_backed,
                    "non-audit format {:?} must not claim cache-backed warm reruns",
                    spec.format
                );
            }
        }
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    /// #12505: the invocation disclosure flips on cache presence, scoped to
    /// the walk's own family: populating the classified family lets the
    /// classified formats claim warm reruns again, while the compact
    /// `repo-exposure-summary-json` walk — which reads only the separate
    /// compact-classified family — keeps the first-run full-price clause
    /// until its own family holds an entry. The same workspace therefore
    /// hears a different stderr line per format, and each format's line
    /// differs between cache-absent and cache-present states.
    #[test]
    fn repo_audit_path_disclosure_is_cache_presence_aware() -> Result<(), String> {
        let root = temp_disclosure_root("presence-aware")?;
        let classified = OutputFormat::RepoExposureMd;
        let compact = OutputFormat::RepoExposureSummaryJson;

        let cold_classified = classified
            .repo_audit_path_disclosure(&root)
            .ok_or("classified disclosure missing")?;
        let cold_compact = compact
            .repo_audit_path_disclosure(&root)
            .ok_or("compact disclosure missing")?;
        assert!(
            cold_classified.contains("first run pays the full walk"),
            "cache-absent classified format must disclose first-run price: {cold_classified}"
        );
        assert!(
            cold_compact.contains("first run pays the full walk"),
            "cache-absent compact format must disclose first-run price: {cold_compact}"
        );
        assert_ne!(
            cold_classified, cold_compact,
            "the two families' cold disclosures must still name their own walk"
        );

        // Populate exactly the classified family through the production
        // layout (`RepoSeamFactCache::at`), not a parallel path guess.
        let classified_entry_dirs =
            crate::analysis::seam_cache::RepoSeamFactCache::at(&root).entry_dirs();
        let (entry_dir, _) = classified_entry_dirs
            .first()
            .ok_or("the cache reports its two entry layers")?;
        std::fs::create_dir_all(entry_dir).map_err(|err| err.to_string())?;
        std::fs::write(entry_dir.join("entry.json"), b"{}").map_err(|err| err.to_string())?;

        let warm_classified = classified
            .repo_audit_path_disclosure(&root)
            .ok_or("classified disclosure missing")?;
        assert!(
            warm_classified.contains("warm reruns reuse the seam-facts cache"),
            "classified format must claim warm reruns once its family has an entry: \
             {warm_classified}"
        );
        let still_cold_compact = compact
            .repo_audit_path_disclosure(&root)
            .ok_or("compact disclosure missing")?;
        assert!(
            still_cold_compact.contains("first run pays the full walk"),
            "compact format must stay on the first-run clause while its own family is empty: \
             {still_cold_compact}"
        );
        assert_ne!(
            warm_classified, still_cold_compact,
            "cache-present and cache-absent runs must hear different disclosures"
        );

        // Populate the compact family; only its own entry flips its clause.
        for (compact_dir, creates_subdirs) in
            crate::analysis::seam_cache::RepoSeamFactCache::at_compact_classified(&root)
                .entry_dirs()
        {
            if !creates_subdirs {
                std::fs::create_dir_all(&compact_dir).map_err(|err| err.to_string())?;
                std::fs::write(compact_dir.join("entry.json"), b"{}")
                    .map_err(|err| err.to_string())?;
                break;
            }
        }
        let warm_compact = compact
            .repo_audit_path_disclosure(&root)
            .ok_or("compact disclosure missing")?;
        assert!(
            warm_compact.contains("warm reruns reuse the seam-facts cache"),
            "compact format must claim warm reruns once its own family has an entry: \
             {warm_compact}"
        );
        assert_ne!(cold_compact, warm_compact);

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    /// Exclusive temp root for the disclosure tests, following the
    /// cache-module test convention (`isolated_dir`): a unique directory
    /// under the system temp dir, removed by the caller after use.
    fn temp_disclosure_root(label: &str) -> Result<std::path::PathBuf, String> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "ripr-disclosure-{label}-{nanos}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|err| format!("mkdir disclosure root: {err}"))?;
        Ok(dir)
    }

    #[test]
    fn repo_format_audit_path_group_is_the_full_repo_analysis_formats() {
        let expected: Vec<&str> = vec![
            "repo-seams-json",
            "repo-seams-md",
            "repo-exposure-json",
            "repo-exposure-summary-json",
            "repo-exposure-md",
            "repo-sarif",
            "agent-seam-packets-json",
            // #5261/#5263 review: the seam-native repo badge formats walk
            // the same full classified inventory as repo-exposure, so they
            // disclose the same cost class.
            "repo-badge-json",
            "repo-badge-shields",
            "repo-badge-plus-json",
            "repo-badge-plus-shields",
        ];
        let mut observed: Vec<&str> = FORMAT_SPECS
            .iter()
            .filter(|spec| spec.is_full_repo_analysis)
            .map(|spec| spec.format.primary_cli_name())
            .collect();
        observed.sort_unstable();
        let mut sorted_expected = expected.clone();
        sorted_expected.sort_unstable();
        assert_eq!(
            observed, sorted_expected,
            "the audit-path disclosure group must be exactly the full-repo analysis formats"
        );
    }

    /// #4945 review: only the classified/compact-classified audit walks read
    /// a seam-facts cache. The raw `repo-seams-*` walks rebuild the corpus
    /// index every run (`inventory_seams_at_with_config` never touches
    /// `RepoSeamFactCache`), so exactly these classified walks may claim
    /// warm reruns.
    #[test]
    fn repo_format_audit_path_cache_backed_group_is_exactly_the_classified_walks() {
        let expected: Vec<&str> = vec![
            "repo-exposure-json",
            "repo-exposure-summary-json",
            "repo-exposure-md",
            "repo-sarif",
            "agent-seam-packets-json",
            // #5261/#5263 review: the seam-native repo badge formats read
            // the same classified cache, so their disclosures may claim warm
            // reruns too.
            "repo-badge-json",
            "repo-badge-shields",
            "repo-badge-plus-json",
            "repo-badge-plus-shields",
        ];
        let mut observed: Vec<&str> = FORMAT_SPECS
            .iter()
            .filter(|spec| spec.is_seam_fact_cache_backed)
            .map(|spec| spec.format.primary_cli_name())
            .collect();
        observed.sort_unstable();
        let mut sorted_expected = expected.clone();
        sorted_expected.sort_unstable();
        assert_eq!(
            observed, sorted_expected,
            "the warm-rerun cache claim must cover exactly the classified walks"
        );
    }

    #[test]
    fn output_format_parse_cli_name_uses_declared_names() {
        for spec in FORMAT_SPECS {
            for cli_name in spec.cli_names {
                assert_eq!(
                    OutputFormat::parse_cli_name(cli_name),
                    Some(spec.format),
                    "CLI name {:?} should parse to {:?}",
                    cli_name,
                    spec.format
                );
            }
        }
        assert_eq!(OutputFormat::parse_cli_name("xml"), None);
    }
}
