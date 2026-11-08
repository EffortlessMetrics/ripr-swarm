//! Bounded full-payload retention for the review-comments consumer.
//! Ranking and omission meaning stay in the parent agent-brief authority.
use super::*;
use std::collections::BTreeSet;

struct OwnedCandidate {
    ordinal: usize,
    entry: ClassifiedSeam,
    why_now: AgentBriefWhyNow,
}

struct OmissionCount {
    kind: AgentBriefOmission,
    count: usize,
    // Ten global named omissions can hide at most ten occurrences of a kind.
    // Eleven positions suffice to recover its first unnamed occurrence.
    first_ordinals: Vec<usize>,
}

pub(crate) struct BoundedAgentBrief<'a> {
    working_set: &'a AgentBriefResolvedWorkingSet,
    normalized: NormalizedAgentBriefWorkingSet<'a>,
    changed_scope: AgentBriefChangedScope<'a>,
    policy: AgentBriefPolicy<'a>,
    requested: usize,
    limit: usize,
    matched_scope: bool,
    visible_candidates: usize,
    candidates: Vec<OwnedCandidate>,
    named_omissions: Vec<(usize, AgentBriefOmission, String)>,
    omission_counts: Vec<OmissionCount>,
    // Only fixed-size identities, never discarded evidence payloads.
    seen_ids: BTreeSet<String>,
}

impl<'a> BoundedAgentBrief<'a> {
    pub(crate) fn new(
        working_set: &'a AgentBriefResolvedWorkingSet,
        requested: usize,
        policy: AgentBriefPolicy<'a>,
    ) -> Result<Self, String> {
        if working_set.seam_id.is_some() {
            return Err(
                "streamed review selection does not accept an explicit seam-id scope; remove the seam-id scope, or request the brief without streaming".into(),
            );
        }
        Ok(Self {
            working_set,
            normalized: NormalizedAgentBriefWorkingSet::new(working_set),
            changed_scope: AgentBriefChangedScope::new(working_set),
            policy,
            requested,
            limit: normalize_requested_max(requested),
            matched_scope: false,
            visible_candidates: 0,
            candidates: Vec::new(),
            named_omissions: Vec::new(),
            omission_counts: Vec::new(),
            seen_ids: BTreeSet::new(),
        })
    }

    /// Seams classified and handed to this consumer so far, whether retained,
    /// omitted by policy or ignored as fallback. A run stopped by its budget
    /// reports this as work done, never as a completed inventory.
    pub(crate) fn observed_seams(&self) -> usize {
        self.seen_ids.len()
    }

    pub(crate) fn observe(&mut self, ordinal: usize, entry: ClassifiedSeam) -> Result<(), String> {
        if !self.seen_ids.insert(entry.seam.id().as_str().to_string()) {
            return Err("duplicate seam identity in streamed review evidence".into());
        }
        let direct = why_now_for(&entry, &self.normalized);
        let why_now = if let Some(why_now) = direct {
            if !self.matched_scope {
                // Fallback never contributes once any scoped seam is represented,
                // even when every matching seam is configured off/non-actionable.
                self.matched_scope = true;
                self.visible_candidates = 0;
                self.candidates.clear();
            }
            if let Some(omission) = AgentBriefOmission::for_entry(&entry, self.policy) {
                self.record_omission(ordinal, &entry, omission)?;
                return Ok(());
            }
            why_now
        } else {
            if self.matched_scope || agent_brief_omission_reason(&entry, self.policy).is_some() {
                return Ok(());
            }
            AgentBriefWhyNow {
                reason: AgentBriefWhyNowReason::RepoActionableFallback,
                confidence: AgentBriefWhyNowConfidence::Low,
                evidence: "no working-set seam matched; selected a repo-actionable seam"
                    .to_string(),
            }
        };
        self.visible_candidates = self
            .visible_candidates
            .checked_add(1)
            .ok_or("streamed review candidate count overflow")?;
        self.candidates.push(OwnedCandidate {
            ordinal,
            entry,
            why_now,
        });
        let policy = self.policy;
        self.candidates.sort_by(|left, right| {
            compare_selected_parts(
                &left.entry,
                &left.why_now,
                &right.entry,
                &right.why_now,
                policy,
            )
            .then(left.ordinal.cmp(&right.ordinal))
        });
        self.candidates.truncate(self.limit);
        Ok(())
    }

    fn record_omission(
        &mut self,
        ordinal: usize,
        entry: &ClassifiedSeam,
        kind: AgentBriefOmission,
    ) -> Result<(), String> {
        let position = self
            .omission_counts
            .iter()
            .position(|item| item.kind == kind);
        let position = match position {
            Some(position) => position,
            None => {
                self.omission_counts.push(OmissionCount {
                    kind,
                    count: 0,
                    first_ordinals: Vec::new(),
                });
                self.omission_counts.len() - 1
            }
        };
        let count = &mut self.omission_counts[position];
        count.count = count
            .count
            .checked_add(1)
            .ok_or("streamed review omission count overflow")?;
        count.first_ordinals.push(ordinal);
        count.first_ordinals.sort_unstable();
        count
            .first_ordinals
            .truncate(AGENT_BRIEF_MAX_NAMED_OMISSIONS + 1);
        self.named_omissions.push((
            ordinal,
            kind,
            format!(
                "{} at {}:{}",
                entry.seam.id().as_str(),
                display_path(entry.seam.file()),
                entry.seam.display_line()
            ),
        ));
        self.named_omissions.sort_by_key(|item| item.0);
        self.named_omissions
            .truncate(AGENT_BRIEF_MAX_NAMED_OMISSIONS);
        Ok(())
    }

    pub(crate) fn selection(&self) -> Result<AgentBriefSelection<'_>, String> {
        let mut warnings = grouped_omission_warnings(
            self.named_omissions
                .iter()
                .map(|item| (item.1, item.2.clone())),
        );
        let mut more = Vec::new();
        for item in &self.omission_counts {
            let named = self
                .named_omissions
                .iter()
                .filter(|named| named.1 == item.kind)
                .count();
            let count = item
                .count
                .checked_sub(named)
                .ok_or("streamed review omission accounting incomplete")?;
            if count == 0 {
                continue;
            }
            let first = item
                .first_ordinals
                .iter()
                .find(|ordinal| {
                    !self
                        .named_omissions
                        .iter()
                        .any(|named| named.0 == **ordinal)
                })
                .copied()
                .ok_or("streamed review omission ordering incomplete")?;
            more.push((first, item.kind.more(count)));
        }
        more.sort_by_key(|item| item.0);
        warnings.extend(more.into_iter().map(|item| item.1));
        if !self.matched_scope {
            warnings.push(if self.candidates.is_empty() {
                format!(
                    "No seams matched the requested scope (source: {}), and no other \
                 agent-actionable seam in the analyzed inventory is visible under the \
                 current config.",
                    self.working_set.source.as_str()
                )
            } else {
                format!(
                    "No seams matched the requested scope (source: {}); showing all \
                 repo-actionable seams instead — this is NOT a scoped result.",
                    self.working_set.source.as_str()
                )
            });
        }
        let omitted = self
            .visible_candidates
            .checked_sub(self.candidates.len())
            .ok_or("streamed review candidate accounting incomplete")?;
        if omitted > 0 {
            warnings.push(format!(
                "{omitted} additional visible seams were omitted by the brief cap"
            ));
        }
        Ok(AgentBriefSelection {
            requested: self.requested,
            returned: self.candidates.len(),
            default: DEFAULT_AGENT_BRIEF_MAX_SEAMS,
            hard_cap: AGENT_BRIEF_HARD_MAX_SEAMS,
            top_seams: self
                .candidates
                .iter()
                .map(|item| AgentBriefSelectedSeam {
                    seam: &item.entry,
                    why_now: item.why_now.clone(),
                })
                .collect(),
            warnings,
            omitted_by_cap: omitted,
            omitted_by_policy: self.omission_counts.iter().map(|item| item.count).sum(),
        })
    }
}

impl crate::analysis::ScopedEvidenceConsumer for BoundedAgentBrief<'_> {
    fn in_first_stage(&self, seam: &RepoSeam) -> bool {
        self.changed_scope.contains(seam)
    }

    fn observe(&mut self, ordinal: usize, entry: ClassifiedSeam) -> Result<(), String> {
        self.observe(ordinal, entry)
    }

    fn first_stage_sufficient(&self) -> bool {
        self.changed_scope.fills_selection(
            self.candidates.iter().map(|item| &item.entry),
            self.requested,
            self.policy,
        )
    }

    fn retained_payloads(&self) -> usize {
        self.candidates.len()
    }

    fn second_stage_scope(&self) -> Option<crate::analysis::SecondStageScope<'_>> {
        // Fallback seams are discarded once any scoped seam is represented, so
        // narrowing is only sound after a first-stage seam matched.
        if !self.matched_scope {
            return None;
        }
        let scope = &self.changed_scope;
        Some(crate::analysis::SecondStageScope {
            keep_without_relation: Box::new(move |seam| scope.can_match_non_fallback(seam, false)),
            relation_pool: Box::new(move |file, line| {
                scope.related_test_can_raise_why_now(file, line)
            }),
            // Without edit attribution every test in a working-set file is in
            // the pool, which per-seam lookups cannot afford on a large test
            // file (#12940), so a non-empty pool then keeps every seam.
            relation_lookup: scope.knows_edited_tests(),
            counts_relation: super::related_test_counts_for_why_now,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::classified;
    use super::*;

    #[test]
    fn observed_seams_counts_every_classified_seam_not_only_retained_ones() -> Result<(), String> {
        let config =
            crate::config::tests_only_parse("[severity.seams]\nweakly_gripped = \"off\"\n")?;
        let working_set =
            AgentBriefResolvedWorkingSet::files(vec![PathBuf::from("src/changed.rs")]);
        let mut sink =
            BoundedAgentBrief::new(&working_set, 10, AgentBriefPolicy::from_config(&config))?;
        assert_eq!(sink.observed_seams(), 0);
        let omitted = classified(
            "src/changed.rs",
            1,
            "changed::a",
            "n > 0",
            SeamGripClass::WeaklyGripped,
        );
        sink.observe(0, omitted)?;
        assert_eq!(sink.observed_seams(), 1);
        assert_eq!(sink.candidates.len(), 0);
        Ok(())
    }

    #[test]
    fn late_hidden_scope_match_discards_fallback_and_counter_overflow_refuses() -> Result<(), String>
    {
        let config = RiprConfig::default();
        let working_set =
            AgentBriefResolvedWorkingSet::files(vec![PathBuf::from("src/changed.rs")]);
        let fallback = classified(
            "src/other.rs",
            1,
            "other::a",
            "n > 0",
            SeamGripClass::WeaklyGripped,
        );
        let hidden = classified(
            "src/changed.rs",
            1,
            "changed::a",
            "n > 0",
            SeamGripClass::StronglyGripped,
        );
        let mut sink =
            BoundedAgentBrief::new(&working_set, 10, AgentBriefPolicy::from_config(&config))?;
        sink.observe(1, fallback)?;
        assert_eq!(sink.candidates.len(), 1);
        sink.observe(0, hidden.clone())?;
        let selection = sink.selection()?;
        assert_eq!(selection.returned, 0);
        assert_eq!(selection.warnings.len(), 1);
        assert!(!selection.warnings[0].contains("fallback"));
        sink.omission_counts[0].count = usize::MAX;
        assert_eq!(
            sink.record_omission(2, &hidden, sink.omission_counts[0].kind)
                .err()
                .as_deref(),
            Some("streamed review omission count overflow")
        );
        sink.visible_candidates = usize::MAX;
        let visible = classified(
            "src/changed.rs",
            3,
            "changed::b",
            "n > 1",
            SeamGripClass::WeaklyGripped,
        );
        assert_eq!(
            sink.observe(3, visible).err().as_deref(),
            Some("streamed review candidate count overflow")
        );
        Ok(())
    }

    #[test]
    fn bounded_counts_every_policy_omission_and_keeps_unmatched_hunk_seam() -> Result<(), String> {
        let config =
            crate::config::tests_only_parse("[severity.seams]\nweakly_gripped = \"off\"\n")?;
        let working_set =
            AgentBriefResolvedWorkingSet::files(vec![PathBuf::from("src/changed.rs")]);
        let mut sink =
            BoundedAgentBrief::new(&working_set, 10, AgentBriefPolicy::from_config(&config))?;
        // More omissions than the named limit: named ones are subtracted
        // when rendering "more" warnings, but the total must still count all.
        let total = AGENT_BRIEF_MAX_NAMED_OMISSIONS + 3;
        for index in 0..total {
            sink.observe(
                index,
                classified(
                    "src/changed.rs",
                    10 + index,
                    &format!("changed::off_{index}"),
                    &format!("n > {index}"),
                    SeamGripClass::WeaklyGripped,
                ),
            )?;
        }
        // A visible seam in the changed file that no hunk/owner matches is
        // retained via the same-file match, never silently skipped.
        sink.observe(
            total,
            classified(
                "src/changed.rs",
                500,
                "changed::visible",
                "n > 500",
                SeamGripClass::Ungripped,
            ),
        )?;
        let selection = sink.selection()?;
        assert_eq!(selection.omitted_by_policy, total);
        assert_eq!(selection.returned, 1);
        assert_eq!(
            selection.top_seams[0].why_now.reason,
            AgentBriefWhyNowReason::SameFileSeam
        );
        Ok(())
    }

    /// Delegates to a real `BoundedAgentBrief` and records which seams reach
    /// evidence. `narrow` false hides the second-stage filter, the baseline.
    struct Recording<'a, 'b> {
        inner: &'b mut BoundedAgentBrief<'a>,
        narrow: bool,
        evaluated: Vec<String>,
    }

    impl crate::analysis::ScopedEvidenceConsumer for Recording<'_, '_> {
        fn in_first_stage(&self, seam: &RepoSeam) -> bool {
            self.inner.in_first_stage(seam)
        }
        fn observe(&mut self, ordinal: usize, entry: ClassifiedSeam) -> Result<(), String> {
            self.evaluated.push(format!(
                "{}::{}",
                entry.seam.file().display(),
                entry.seam.owner()
            ));
            self.inner.observe(ordinal, entry)
        }
        fn first_stage_sufficient(&self) -> bool {
            self.inner.first_stage_sufficient()
        }
        fn retained_payloads(&self) -> usize {
            self.inner.retained_payloads()
        }
        fn second_stage_scope(&self) -> Option<crate::analysis::SecondStageScope<'_>> {
            if self.narrow {
                self.inner.second_stage_scope()
            } else {
                None
            }
        }
    }

    const LIB_PLAIN: &str = "pub fn eligible(n: i32) -> bool { n >= 10 }\n";
    const LIB_WITH_TEST: &str = "pub fn eligible(n: i32) -> bool { n >= 10 }\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn boundary() { assert!(eligible(10)); }\n}\n";
    const CALLER: &str =
        "pub fn caller(n: i32) -> bool { if eligible(n) { n > 1 } else { n < 0 } }\n";

    type FixtureRun = (Vec<String>, Vec<(String, String)>, Vec<String>);

    /// Returns (evaluated seams, selected `(owner, why_now)`, warnings).
    fn run_fixture(label: &str, lib: &str, narrow: bool) -> Result<FixtureRun, String> {
        run_fixture_at(label, lib, narrow, true)
    }

    /// `hit` false points the changed line and owner at nothing, so no
    /// first-stage seam matches and every seam is a fallback.
    fn run_fixture_at(
        label: &str,
        lib: &str,
        narrow: bool,
        hit: bool,
    ) -> Result<FixtureRun, String> {
        run_fixture_with(label, lib, narrow, hit, &[], None)
    }

    /// `extra_lines` adds changed lines in `src/lib.rs`; `edited_headers`
    /// sets edit attribution (`None` leaves it unknown).
    fn run_fixture_with(
        label: &str,
        lib: &str,
        narrow: bool,
        hit: bool,
        extra_lines: &[usize],
        edited_headers: Option<&[usize]>,
    ) -> Result<FixtureRun, String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-bounded-{label}-{}-{narrow}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        for (name, text) in [
            ("src/lib.rs", format!("mod caller;\n{lib}")),
            ("src/caller.rs", CALLER.to_string()),
        ] {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().ok_or("no parent")?)
                .map_err(|e| e.to_string())?;
            std::fs::write(&path, text).map_err(|e| e.to_string())?;
        }
        let mut working_set =
            AgentBriefResolvedWorkingSet::files(vec![PathBuf::from("src/lib.rs")]);
        let (line, owner) = if hit { (2, "eligible") } else { (90, "absent") };
        working_set.changed_lines = vec![AgentBriefLine::new("src/lib.rs", line)];
        working_set.changed_owners = vec![AgentBriefChangedOwner::new("src/lib.rs", line, owner)];
        working_set.changed_lines.extend(
            extra_lines
                .iter()
                .map(|extra| AgentBriefLine::new("src/lib.rs", *extra)),
        );
        working_set.edited_function_headers = edited_headers.map(|headers| {
            headers
                .iter()
                .map(|header| AgentBriefLine::new("src/lib.rs", *header))
                .collect()
        });
        let config = RiprConfig::default();
        let mut brief =
            BoundedAgentBrief::new(&working_set, 10, AgentBriefPolicy::from_config(&config))?;
        let mut recording = Recording {
            inner: &mut brief,
            narrow,
            evaluated: Vec::new(),
        };
        crate::analysis::inventory_diff_scoped_streamed_seams_at_with_config(
            &root,
            &config,
            &[PathBuf::from("src/lib.rs")],
            &["eligible".to_string()],
            &mut recording,
        )?;
        let evaluated = recording.evaluated;
        let selection = brief.selection()?;
        let selected = selection
            .top_seams
            .iter()
            .map(|item| {
                (
                    item.seam.seam.owner().to_string(),
                    format!("{:?}", item.why_now.reason),
                )
            })
            .collect();
        let warnings = selection.warnings.clone();
        let _ = std::fs::remove_dir_all(&root);
        Ok((evaluated, selected, warnings))
    }

    #[test]
    fn unmatchable_caller_seam_is_not_evaluated_but_selection_is_unchanged() -> Result<(), String> {
        let (wide, wide_sel, wide_warn) = run_fixture("neg-wide", LIB_PLAIN, false)?;
        let (narrow, narrow_sel, narrow_warn) = run_fixture("neg-narrow", LIB_PLAIN, true)?;
        assert!(
            wide.iter().any(|id| id.contains("caller.rs")),
            "baseline must evaluate the caller-file seam: {wide:?}"
        );
        assert!(
            narrow.iter().any(|id| id.ends_with("::eligible")),
            "first-stage seam is still evaluated: {narrow:?}"
        );
        assert!(
            !narrow.iter().any(|id| id.contains("caller.rs")),
            "caller seam cannot match a non-fallback why-now: {narrow:?}"
        );
        assert!(narrow.len() < wide.len());
        assert!(!narrow_sel.is_empty());
        assert_eq!(wide_sel, narrow_sel);
        assert_eq!(wide_warn, narrow_warn);
        Ok(())
    }

    #[test]
    fn caller_seam_stays_evaluated_when_a_working_set_file_holds_tests() -> Result<(), String> {
        let (wide, wide_sel, wide_warn) = run_fixture("pos-wide", LIB_WITH_TEST, false)?;
        let (narrow, narrow_sel, narrow_warn) = run_fixture("pos-narrow", LIB_WITH_TEST, true)?;
        assert!(
            narrow.iter().any(|id| id.contains("caller.rs")),
            "a related test in a working-set file keeps the related-test routes open: {narrow:?}"
        );
        assert_eq!(wide.len(), narrow.len());
        assert_eq!(wide_sel, narrow_sel);
        assert_eq!(wide_warn, narrow_warn);
        Ok(())
    }

    #[test]
    fn predicate_keeps_changed_same_file_and_related_routes_only() {
        let mut working_set =
            AgentBriefResolvedWorkingSet::files(vec![PathBuf::from("src/lib.rs")]);
        working_set.changed_lines = vec![AgentBriefLine::new("src/lib.rs", 2)];
        let scope = AgentBriefChangedScope::new(&working_set);
        let same_file = classified(
            "src/lib.rs",
            9,
            "lib::other",
            "n > 0",
            SeamGripClass::WeaklyGripped,
        );
        let caller = classified(
            "src/caller.rs",
            1,
            "caller::a",
            "n > 0",
            SeamGripClass::WeaklyGripped,
        );
        assert!(scope.can_match_non_fallback(&same_file.seam, false));
        assert!(!scope.can_match_non_fallback(&caller.seam, false));
        assert!(scope.can_match_non_fallback(&caller.seam, true));
        // Without edit attribution every test in a working-set file may
        // raise the file-level why-now; a test outside the working set only
        // through a changed line on its header or assertion window.
        assert!(!scope.knows_edited_tests());
        assert!(!scope.related_test_can_raise_why_now(Path::new("tests/other.rs"), 1));
        assert!(scope.related_test_can_raise_why_now(Path::new("./src/lib.rs"), 40));
    }

    #[test]
    fn edited_test_attribution_limits_the_relation_pool_to_edited_tests() {
        let mut working_set = AgentBriefResolvedWorkingSet::base(
            "main",
            vec![
                AgentBriefLine::new("src/lib.rs", 2),
                AgentBriefLine::new("tests/t.rs", 30),
            ],
        )
        .with_edited_function_headers(Some(vec![
            AgentBriefLine::new("src/lib.rs", 1),
            AgentBriefLine::new("tests/t.rs", 10),
        ]));
        working_set
            .files
            .push(PathBuf::from("tests/listed_only.rs"));
        let scope = AgentBriefChangedScope::new(&working_set);
        assert!(scope.knows_edited_tests());
        // Edited test (its body holds a changed line), by header line.
        assert!(scope.related_test_can_raise_why_now(Path::new("tests/t.rs"), 10));
        // Untouched test in a changed test file: no route.
        assert!(!scope.related_test_can_raise_why_now(Path::new("tests/t.rs"), 50));
        // Changed line on the header, or inside the assertion window after
        // it, still admits the test (exact-line and assertion-near routes).
        assert!(scope.related_test_can_raise_why_now(Path::new("tests/t.rs"), 30));
        assert!(scope.related_test_can_raise_why_now(Path::new("tests/t.rs"), 18));
        assert!(!scope.related_test_can_raise_why_now(Path::new("tests/t.rs"), 17));
        // A working-set file with no changed lines keeps the file route open.
        assert!(scope.related_test_can_raise_why_now(Path::new("tests/listed_only.rs"), 99));
        assert!(!scope.related_test_can_raise_why_now(Path::new("tests/other.rs"), 10));
    }

    /// `src/lib.rs` (after the fixture's `mod caller;` line): `eligible` on
    /// line 2, test `boundary` on line 7, test `via_caller` on line 9 with
    /// its assertion on line 11, past the 12-line assertion window of
    /// `boundary` only when the changed line is beyond line 19.
    const LIB_WITH_CALLER_TEST: &str = "pub fn eligible(n: i32) -> bool { n >= 10 }\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn boundary() { assert!(eligible(10)); }\n    #[test]\n    fn via_caller() {\n        let n = 3;\n        assert!(crate::caller::caller(n));\n    }\n}\n";

    #[test]
    fn caller_seam_related_only_to_an_untouched_test_is_skipped() -> Result<(), String> {
        // The diff edits `eligible` only; `via_caller` (line 9) relates to the
        // caller seam but is untouched, so no related-test why-now can name
        // the caller seam and it is not evaluated.
        let headers: &[usize] = &[2];
        let (wide, wide_sel, wide_warn) = run_fixture_with(
            "ut-wide",
            LIB_WITH_CALLER_TEST,
            false,
            true,
            &[],
            Some(headers),
        )?;
        let (narrow, narrow_sel, narrow_warn) = run_fixture_with(
            "ut-narrow",
            LIB_WITH_CALLER_TEST,
            true,
            true,
            &[],
            Some(headers),
        )?;
        assert!(
            wide.iter().any(|id| id.contains("caller.rs")),
            "baseline must evaluate the caller-file seam: {wide:?}"
        );
        assert!(
            !narrow.iter().any(|id| id.contains("caller.rs")),
            "an untouched related test must not keep the caller seam: {narrow:?}"
        );
        assert!(
            narrow.iter().any(|id| id.ends_with("::eligible")),
            "first-stage seam is still evaluated: {narrow:?}"
        );
        assert_eq!(wide_sel, narrow_sel);
        assert_eq!(wide_warn, narrow_warn);
        Ok(())
    }

    /// `LIB_WITH_CALLER_TEST` with `boundary` (line 7) replaced by a test that
    /// calls and asserts nothing the caller seam names.
    const LIB_WITH_UNRELATED_EDITED_TEST: &str = "pub fn eligible(n: i32) -> bool { n >= 10 }\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn arithmetic() { assert_eq!(2 + 2, 4); }\n    #[test]\n    fn via_caller() {\n        let n = 3;\n        assert!(crate::caller::caller(n));\n    }\n}\n";

    #[test]
    fn caller_seam_related_only_to_a_test_outside_a_nonempty_pool_is_skipped() -> Result<(), String>
    {
        // The diff edits `eligible` (line 2) and the test `arithmetic` (line
        // 7), so the pool holds `arithmetic`. The caller seam relates to the
        // untouched `via_caller` (header 9, after every changed line), and to
        // `arithmetic` at most by proximity, so the relation lookup must
        // reject it and selection must not change.
        let headers: &[usize] = &[2, 7];
        let (wide, wide_sel, wide_warn) = run_fixture_with(
            "np-wide",
            LIB_WITH_UNRELATED_EDITED_TEST,
            false,
            true,
            &[7],
            Some(headers),
        )?;
        let (narrow, narrow_sel, narrow_warn) = run_fixture_with(
            "np-narrow",
            LIB_WITH_UNRELATED_EDITED_TEST,
            true,
            true,
            &[7],
            Some(headers),
        )?;
        assert!(
            wide.iter().any(|id| id.contains("caller.rs")),
            "baseline must evaluate the caller-file seam: {wide:?}"
        );
        assert!(
            !narrow.iter().any(|id| id.contains("caller.rs")),
            "a pool test the caller seam does not relate to must not keep it: {narrow:?}"
        );
        assert_eq!(wide_sel, narrow_sel);
        assert_eq!(wide_warn, narrow_warn);
        Ok(())
    }

    #[test]
    fn caller_seam_related_to_an_edited_test_stays_evaluated() -> Result<(), String> {
        // The diff also edits line 10 inside `via_caller` (header line 9), so
        // the caller seam relates to an edited test and must be evaluated;
        // selection names it through the related-test why-now.
        let headers: &[usize] = &[2, 9];
        let (wide, wide_sel, wide_warn) = run_fixture_with(
            "et-wide",
            LIB_WITH_CALLER_TEST,
            false,
            true,
            &[10],
            Some(headers),
        )?;
        let (narrow, narrow_sel, narrow_warn) = run_fixture_with(
            "et-narrow",
            LIB_WITH_CALLER_TEST,
            true,
            true,
            &[10],
            Some(headers),
        )?;
        assert!(
            wide.iter().any(|id| id.contains("caller.rs")),
            "baseline must evaluate the caller-file seam: {wide:?}"
        );
        assert!(
            narrow.iter().any(|id| id.contains("caller.rs")),
            "a related edited test keeps the caller seam: {narrow:?}"
        );
        assert!(
            narrow_sel
                .iter()
                .any(|(owner, why)| owner.ends_with("caller") && why.contains("Related")),
            "the caller seam is selected through its edited related test: {narrow_sel:?}"
        );
        assert_eq!(wide_sel, narrow_sel);
        assert_eq!(wide_warn, narrow_warn);
        Ok(())
    }

    #[test]
    fn without_a_first_stage_match_nothing_is_skipped() -> Result<(), String> {
        let (wide, wide_sel, wide_warn) = run_fixture_at("nm-wide", LIB_PLAIN, false, false)?;
        let (narrow, narrow_sel, narrow_warn) =
            run_fixture_at("nm-narrow", LIB_PLAIN, true, false)?;
        assert!(
            narrow.iter().any(|id| id.contains("caller.rs")),
            "with no scoped match every seam is a fallback and must be evaluated: {narrow:?}"
        );
        assert_eq!(wide, narrow);
        assert_eq!(wide_sel, narrow_sel);
        assert_eq!(wide_warn, narrow_warn);
        Ok(())
    }
}
